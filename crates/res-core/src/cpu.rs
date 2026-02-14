use crate::mapper::{Mapper, MapperError, NromMapper};
use crate::apu::Apu;
use crate::opcodes;
use crate::ppu::Ppu;
use crate::rom::{Mirroring, Rom};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

bitflags! {
  pub struct CpuFlags: u8 {
    const CARRY             = 0b00000001;
    const ZERO              = 0b00000010;
    const INTERRUPT_DISABLE = 0b00000100;
    const DECIMAL_MODE      = 0b00001000;
    const BREAK             = 0b00010000;
    const BREAK2            = 0b00100000;
    const OVERFLOW          = 0b01000000;
    const NEGATIV           = 0b10000000;
  }
}

const STACK: u16 = 0x0100;
const STACK_RESET: u8 = 0xfd;
const NMI_VECTOR: u16 = 0xFFFA;
const RESET_VECTOR: u16 = 0xFFFC;
const IRQ_BRK_VECTOR: u16 = 0xFFFE;

pub struct CPU {
    pub register_a: u8,
    pub register_x: u8,
    pub register_y: u8,
    pub status: CpuFlags,
    pub program_counter: u16,
    pub stack_pointer: u8,
    cycles: u64,
    memory: [u8; 0x10000],
    apu: Apu,
    ppu: RefCell<Ppu>,
    mapper: Option<Rc<RefCell<dyn Mapper>>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CpuError {
    UnsupportedOpcode { opcode: u8, pc: u16 },
}

#[derive(Debug, PartialEq, Eq)]
pub enum CpuLoadError {
    InvalidPrgSize(usize),
    UnsupportedMapper(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceState {
    pub pc: u16,
    pub opcode: u8,
    pub mnemonic: &'static str,
    pub register_a: u8,
    pub register_x: u8,
    pub register_y: u8,
    pub status: u8,
    pub stack_pointer: u8,
}

impl TraceState {
    pub fn to_log_line(&self) -> String {
        format!(
            "PC:{:04X} OPC:{:02X} {:<3} A:{:02X} X:{:02X} Y:{:02X} P:{:08b} SP:{:02X}",
            self.pc,
            self.opcode,
            self.mnemonic,
            self.register_a,
            self.register_x,
            self.register_y,
            self.status,
            self.stack_pointer
        )
    }
}

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum AddressingMode {
    Immediate,
    ZeroPage,
    ZeroPage_X,
    ZeroPage_Y,
    Absolute,
    Absolute_X,
    Absolute_Y,
    Indirect_X,
    Indirect_Y,
    NoneAddressing,
}

pub trait Mem {
    fn mem_read(&self, addr: u16) -> u8;

    fn mem_write(&mut self, addr: u16, data: u8);

    fn mem_read_u16(&self, pos: u16) -> u16 {
        let lo = self.mem_read(pos) as u16;
        let hi = self.mem_read(pos.wrapping_add(1)) as u16;
        (hi << 8) | (lo as u16)
    }

    fn mem_write_u16(&mut self, pos: u16, data: u16) {
        let hi = (data >> 8) as u8;
        let lo = (data & 0xff) as u8;
        self.mem_write(pos, lo);
        self.mem_write(pos.wrapping_add(1), hi);
    }
}

impl Mem for CPU {
    fn mem_read(&self, addr: u16) -> u8 {
        match addr {
            0x4000..=0x4017 => self.apu.read_register(addr),
            0x2000..=0x3FFF => {
                let reg = 0x2000 + ((addr - 0x2000) % 8);
                self.ppu.borrow_mut().read_register(reg)
            }
            0x8000..=0xFFFF => {
                if let Some(mapper) = &self.mapper {
                    if let Some(data) = mapper.borrow().cpu_read(addr) {
                        return data;
                    }
                }
                self.memory[addr as usize]
            }
            _ => self.memory[addr as usize],
        }
    }

    fn mem_write(&mut self, addr: u16, data: u8) {
        match addr {
            0x4000..=0x4017 => self.apu.write_register(addr, data),
            0x2000..=0x3FFF => {
                let reg = 0x2000 + ((addr - 0x2000) % 8);
                self.ppu.borrow_mut().write_register(reg, data);
            }
            0x8000..=0xFFFF => {
                if let Some(mapper) = &self.mapper {
                    if mapper.borrow_mut().cpu_write(addr, data) {
                        return;
                    }
                }
                self.memory[addr as usize] = data;
            }
            _ => self.memory[addr as usize] = data,
        }
    }
}

impl CPU {
    pub fn new() -> Self {
        CPU {
            register_a: 0,
            register_x: 0,
            register_y: 0,
            status: CpuFlags::from_bits_truncate(0b100100),
            program_counter: 0,
            stack_pointer: STACK_RESET,
            cycles: 0,
            memory: [0; 0x10000],
            apu: Apu::new(),
            ppu: RefCell::new(Ppu::new(Mirroring::Horizontal)),
            mapper: None,
        }
    }

    pub fn set_ppu_mirroring(&mut self, mirroring: Mirroring) {
        self.ppu.borrow_mut().set_mirroring(mirroring);
    }

    pub fn load_cartridge(&mut self, rom: Rom) -> Result<(), CpuLoadError> {
        if rom.mapper != 0 {
            return Err(CpuLoadError::UnsupportedMapper(rom.mapper));
        }

        let mapper = NromMapper::new(rom.prg_rom, rom.chr_rom, rom.has_chr_ram)
            .map_err(|err| match err {
                MapperError::InvalidPrgSize(size) => CpuLoadError::InvalidPrgSize(size),
            })?;
        let shared_mapper: Rc<RefCell<dyn Mapper>> = Rc::new(RefCell::new(mapper));

        self.set_ppu_mirroring(rom.mirroring);
        self.ppu.borrow_mut().set_mapper(Some(shared_mapper.clone()));
        self.mapper = Some(shared_mapper);
        Ok(())
    }

    fn did_page_cross(&self, mode: &AddressingMode) -> bool {
        match mode {
            AddressingMode::Absolute_X => {
                let base = self.mem_read_u16(self.program_counter);
                (base & 0xFF00) != (base.wrapping_add(self.register_x as u16) & 0xFF00)
            }
            AddressingMode::Absolute_Y => {
                let base = self.mem_read_u16(self.program_counter);
                (base & 0xFF00) != (base.wrapping_add(self.register_y as u16) & 0xFF00)
            }
            AddressingMode::Indirect_Y => {
                let base = self.mem_read(self.program_counter);
                let lo = self.mem_read(base as u16);
                let hi = self.mem_read((base as u8).wrapping_add(1) as u16);
                let deref_base = (hi as u16) << 8 | (lo as u16);

                (deref_base & 0xFF00) != (deref_base.wrapping_add(self.register_y as u16) & 0xFF00)
            }
            _ => false,
        }
    }

    fn opcode_has_page_cross_penalty(code: u8) -> bool {
        matches!(
            code,
            0xbd | 0xb9
                | 0xb1
                | 0x7d
                | 0x79
                | 0x71
                | 0xfd
                | 0xf9
                | 0xf1
                | 0x3d
                | 0x39
                | 0x31
                | 0x5d
                | 0x59
                | 0x51
                | 0x1d
                | 0x19
                | 0x11
                | 0xdd
                | 0xd9
                | 0xd1
                | 0xbe
                | 0xbc
        )
    }

    fn get_operand_address(&self, mode: &AddressingMode) -> u16 {
        match mode {
            AddressingMode::Immediate => self.program_counter,
            AddressingMode::ZeroPage => self.mem_read(self.program_counter) as u16,
            AddressingMode::Absolute => self.mem_read_u16(self.program_counter),
            AddressingMode::ZeroPage_X => {
                let pos = self.mem_read(self.program_counter);

                pos.wrapping_add(self.register_x) as u16
            }
            AddressingMode::ZeroPage_Y => {
                let pos = self.mem_read(self.program_counter);

                pos.wrapping_add(self.register_y) as u16
            }
            AddressingMode::Absolute_X => {
                let base = self.mem_read_u16(self.program_counter);

                base.wrapping_add(self.register_x as u16)
            }
            AddressingMode::Absolute_Y => {
                let base = self.mem_read_u16(self.program_counter);

                base.wrapping_add(self.register_y as u16)
            }
            AddressingMode::Indirect_X => {
                let base = self.mem_read(self.program_counter);

                let ptr: u8 = base.wrapping_add(self.register_x);
                let lo = self.mem_read(ptr as u16);
                let hi = self.mem_read(ptr.wrapping_add(1) as u16);
                (hi as u16) << 8 | (lo as u16)
            }
            AddressingMode::Indirect_Y => {
                let base = self.mem_read(self.program_counter);

                let lo = self.mem_read(base as u16);
                let hi = self.mem_read((base as u8).wrapping_add(1) as u16);
                let deref_base = (hi as u16) << 8 | (lo as u16);

                deref_base.wrapping_add(self.register_y as u16)
            }
            AddressingMode::NoneAddressing => {
                panic!("mode {:?} is not supported", mode)
            }
        }
    }

    fn lda(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(&mode);
        let value = self.mem_read(addr);

        self.set_register_a(value);
    }

    fn ldx(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.register_x = data;
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn ldy(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.register_y = data;
        self.update_zero_and_negative_flags(self.register_y);
    }

    fn tax(&mut self) {
        self.register_x = self.register_a;
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn inx(&mut self) {
        self.register_x = self.register_x.wrapping_add(1);
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn iny(&mut self) {
        self.register_y = self.register_y.wrapping_add(1);
        self.update_zero_and_negative_flags(self.register_y);
    }

    fn sta(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        self.mem_write(addr, self.register_a);
    }

    fn and(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.set_register_a(data & self.register_a);
    }

    fn eor(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.set_register_a(data ^ self.register_a);
    }

    fn ora(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.set_register_a(data | self.register_a);
    }

    fn sbc(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(&mode);
        let data = self.mem_read(addr);
        self.add_to_register_a(((data as i8).wrapping_neg().wrapping_sub(1)) as u8);
    }

    fn adc(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let value = self.mem_read(addr);
        self.add_to_register_a(value);
    }

    fn asl_accumulator(&mut self) {
        let mut data = self.register_a;
        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data <<= 1;
        self.set_register_a(data)
    }

    fn asl(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data <<= 1;
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    fn lsr_accumulator(&mut self) {
        let mut data = self.register_a;
        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data >>= 1;
        self.set_register_a(data);
    }

    fn lsr(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }
        data >>= 1;
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    fn rol(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        let old_carry = self.status.contains(CpuFlags::CARRY);

        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }

        data <<= 1;

        if old_carry {
            data |= 1;
        }
        self.mem_write(addr, data);
        self.update_negative_flags(data);
        data
    }

    fn rol_accumulator(&mut self) {
        let mut data = self.register_a;
        let old_carry = self.status.contains(CpuFlags::CARRY);

        if data >> 7 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }

        data <<= 1;
        if old_carry {
            data |= 1;
        }
        self.set_register_a(data);
    }

    fn ror(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        let old_carry = self.status.contains(CpuFlags::CARRY);

        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }

        data >>= 1;
        if old_carry {
            data |= 0b10000000;
        }

        self.mem_write(addr, data);
        self.update_negative_flags(data);
        data
    }

    fn ror_accumulator(&mut self) {
        let mut data = self.register_a;
        let old_carry = self.status.contains(CpuFlags::CARRY);

        if data & 1 == 1 {
            self.set_carry_flag();
        } else {
            self.clear_carry_flag();
        }

        data >>= 1;
        if old_carry {
            data |= 0b10000000
        }
        self.set_register_a(data);
    }

    fn inc(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        data = data.wrapping_add(1);
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    fn dec(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        data = data.wrapping_sub(1);
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    fn dex(&mut self) {
        self.register_x = self.register_x.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn dey(&mut self) {
        self.register_y = self.register_y.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.register_y);
    }

    fn branch(&mut self, condition: bool) -> (bool, bool) {
        if !condition {
            return (false, false);
        }

        let jump: i8 = self.mem_read(self.program_counter) as i8;
        let base = self.program_counter.wrapping_add(1);
        let jump_addr = base.wrapping_add(jump as u16);

        let page_crossed = (base & 0xFF00) != (jump_addr & 0xFF00);
        self.program_counter = jump_addr;
        (true, page_crossed)
    }

    fn set_carry_flag(&mut self) {
        self.status.insert(CpuFlags::CARRY)
    }

    fn clear_carry_flag(&mut self) {
        self.status.remove(CpuFlags::CARRY)
    }

    fn set_register_a(&mut self, value: u8) {
        self.register_a = value;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn add_to_register_a(&mut self, data: u8) {
        let sum = self.register_a as u16
            + data as u16
            + (if self.status.contains(CpuFlags::CARRY) {
                1
            } else {
                0
            }) as u16;

        let carry = sum > 0xff;

        if carry {
            self.status.insert(CpuFlags::CARRY);
        } else {
            self.status.remove(CpuFlags::CARRY);
        }

        let result = sum as u8;

        if (data ^ result) & (result ^ self.register_a) & 0x80 != 0 {
            self.status.insert(CpuFlags::OVERFLOW);
        } else {
            self.status.remove(CpuFlags::OVERFLOW);
        }

        self.set_register_a(result);
    }

    fn update_zero_and_negative_flags(&mut self, result: u8) {
        if result == 0 {
            self.status.insert(CpuFlags::ZERO);
        } else {
            self.status.remove(CpuFlags::ZERO);
        }

        if result >> 7 == 1 {
            self.status.insert(CpuFlags::NEGATIV);
        } else {
            self.status.remove(CpuFlags::NEGATIV);
        }
    }

    fn update_negative_flags(&mut self, result: u8) {
        if result >> 7 == 1 {
            self.status.insert(CpuFlags::NEGATIV)
        } else {
            self.status.remove(CpuFlags::NEGATIV)
        }
    }

    fn stack_push(&mut self, data: u8) {
        self.mem_write((STACK as u16) + self.stack_pointer as u16, data);
        self.stack_pointer = self.stack_pointer.wrapping_sub(1);
    }

    fn stack_pop(&mut self) -> u8 {
        self.stack_pointer = self.stack_pointer.wrapping_add(1);
        self.mem_read((STACK as u16) + self.stack_pointer as u16)
    }

    fn stack_push_u16(&mut self, data: u16) {
        let hi = (data >> 8) as u8;
        let lo = (data & 0xff) as u8;
        self.stack_push(hi);
        self.stack_push(lo);
    }

    fn stack_pop_u16(&mut self) -> u16 {
        let lo = self.stack_pop() as u16;
        let hi = self.stack_pop() as u16;

        hi << 8 | lo
    }

    fn bit(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        let and = self.register_a & data;
        if and == 0 {
            self.status.insert(CpuFlags::ZERO);
        } else {
            self.status.remove(CpuFlags::ZERO);
        }

        self.status.set(CpuFlags::NEGATIV, data & 0b10000000 > 0);
        self.status.set(CpuFlags::OVERFLOW, data & 0b01000000 > 0);
    }

    fn plp(&mut self) {
        self.status.bits = self.stack_pop();
        self.status.remove(CpuFlags::BREAK);
        self.status.insert(CpuFlags::BREAK2);
    }

    fn php(&mut self) {
        let mut flags = self.status.clone();
        flags.insert(CpuFlags::BREAK);
        flags.insert(CpuFlags::BREAK2);
        self.stack_push(flags.bits());
    }

    fn pla(&mut self) {
        let data = self.stack_pop();
        self.set_register_a(data);
    }

    fn compare(&mut self, mode: &AddressingMode, compare_with: u8) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        if data <= compare_with {
            self.status.insert(CpuFlags::CARRY);
        } else {
            self.status.remove(CpuFlags::CARRY);
        }

        self.update_zero_and_negative_flags(compare_with.wrapping_sub(data));
    }

    pub fn load(&mut self, program: Vec<u8>) {
        self.memory[0x0600..(0x0600 + program.len())].copy_from_slice(&program[..]);
        self.mem_write_u16(RESET_VECTOR, 0x0600);
    }

    pub fn load_prg_rom(&mut self, prg_rom: &[u8]) -> Result<(), CpuLoadError> {
        self.mapper = None;
        self.ppu.borrow_mut().set_mapper(None);

        match prg_rom.len() {
            0x4000 => {
                self.memory[0x8000..0xC000].copy_from_slice(prg_rom);
                self.memory[0xC000..0x10000].copy_from_slice(prg_rom);
                Ok(())
            }
            0x8000 => {
                self.memory[0x8000..0x10000].copy_from_slice(prg_rom);
                Ok(())
            }
            size => Err(CpuLoadError::InvalidPrgSize(size)),
        }
    }

    pub fn reset(&mut self) {
        self.register_a = 0;
        self.register_x = 0;
        self.register_y = 0;
        self.stack_pointer = STACK_RESET;
        self.status = CpuFlags::from_bits_truncate(0b100100);
        self.cycles = 0;

        self.program_counter = self.mem_read_u16(RESET_VECTOR);
    }

    pub fn total_cycles(&self) -> u64 {
        self.cycles
    }

    fn push_interrupt_state(&mut self, break_flag: bool) {
        self.stack_push_u16(self.program_counter);

        let mut status = self.status;
        status.set(CpuFlags::BREAK, break_flag);
        status.insert(CpuFlags::BREAK2);
        self.stack_push(status.bits());
    }

    pub fn trigger_nmi(&mut self) {
        self.push_interrupt_state(false);
        self.status.insert(CpuFlags::INTERRUPT_DISABLE);
        self.program_counter = self.mem_read_u16(NMI_VECTOR);
    }

    pub fn trigger_irq(&mut self) -> bool {
        if self.status.contains(CpuFlags::INTERRUPT_DISABLE) {
            return false;
        }

        self.push_interrupt_state(false);
        self.status.insert(CpuFlags::INTERRUPT_DISABLE);
        self.program_counter = self.mem_read_u16(IRQ_BRK_VECTOR);
        true
    }

    pub fn trigger_brk(&mut self) {
        self.push_interrupt_state(true);
        self.status.insert(CpuFlags::INTERRUPT_DISABLE);
        self.program_counter = self.mem_read_u16(IRQ_BRK_VECTOR);
    }

    pub fn run(&mut self) {
        self.run_with_callback(|_| {});
    }

    pub fn load_and_run(&mut self, program: Vec<u8>) {
        self.load(program);
        self.reset();
        self.run();
    }

    pub fn run_with_callback<F>(&mut self, mut callback: F)
    where
        F: FnMut(&mut CPU),
    {
        if let Err(err) = self.try_run_with_callback(&mut callback) {
            panic!("CPU halted with error: {:?}", err);
        }
    }

    pub fn run_with_trace<F>(&mut self, mut callback: F)
    where
        F: FnMut(TraceState),
    {
        if let Err(err) = self.try_run_with_trace(&mut callback) {
            panic!("CPU halted with error: {:?}", err);
        }
    }

    pub fn try_run_with_trace<F>(&mut self, callback: &mut F) -> Result<(), CpuError>
    where
        F: FnMut(TraceState),
    {
        self.try_run_with_callback(&mut |cpu| callback(cpu.capture_trace_state()))
    }

    pub fn current_trace_state(&self) -> TraceState {
        self.capture_trace_state()
    }

    fn capture_trace_state(&self) -> TraceState {
        let opcode = self.mem_read(self.program_counter);
        let mnemonic = opcodes::OPCODES_MAP
            .get(&opcode)
            .map_or("???", |op| op.mnemonic);

        TraceState {
            pc: self.program_counter,
            opcode,
            mnemonic,
            register_a: self.register_a,
            register_x: self.register_x,
            register_y: self.register_y,
            status: self.status.bits(),
            stack_pointer: self.stack_pointer,
        }
    }

    pub fn try_run_with_callback<F>(&mut self, callback: &mut F) -> Result<(), CpuError>
    where
        F: FnMut(&mut CPU),
    {
        let ref opcodes: &HashMap<u8, &'static opcodes::OpCode> = &(*opcodes::OPCODES_MAP);

        loop {
            let code = self.mem_read(self.program_counter);
            let opcode_pc = self.program_counter;
            self.program_counter += 1;
            let program_counter_state = self.program_counter;

            let opcode = match opcodes.get(&code) {
                Some(opcode) => opcode,
                None => {
                    return Err(CpuError::UnsupportedOpcode {
                        opcode: code,
                        pc: opcode_pc,
                    })
                }
            };

            let mut extra_cycles: u64 = 0;

            if CPU::opcode_has_page_cross_penalty(code) && self.did_page_cross(&opcode.mode) {
                extra_cycles += 1;
            }

            match code {
                0xa9 | 0xa5 | 0xb5 | 0xad | 0xbd | 0xb9 | 0xa1 | 0xb1 => {
                    self.lda(&opcode.mode);
                }

                0x85 | 0x95 | 0x8d | 0x9d | 0x99 | 0x81 | 0x91 => {
                    self.sta(&opcode.mode);
                }

                0xd8 => self.status.remove(CpuFlags::DECIMAL_MODE),
                0x58 => self.status.remove(CpuFlags::INTERRUPT_DISABLE),
                0xb8 => self.status.remove(CpuFlags::OVERFLOW),
                0x18 => self.clear_carry_flag(),
                0x38 => self.set_carry_flag(),
                0x78 => self.status.insert(CpuFlags::INTERRUPT_DISABLE),
                0xf8 => self.status.insert(CpuFlags::DECIMAL_MODE),

                0xAA => self.tax(),
                0xE8 => self.inx(),
                0x00 => {
                    self.cycles += opcode.cycles as u64;
                    return Ok(());
                }
                0x48 => self.stack_push(self.register_a),
                0x68 => {
                    self.pla();
                }
                0x08 => {
                    self.php();
                }
                0x28 => {
                    self.plp();
                }
                0xea => {
                    // do nothing
                }
                0x69 | 0x65 | 0x75 | 0x6d | 0x7d | 0x79 | 0x61 | 0x71 => {
                    self.adc(&opcode.mode);
                }
                0xe9 | 0xe5 | 0xf5 | 0xed | 0xfd | 0xf9 | 0xe1 | 0xf1 => {
                    self.sbc(&opcode.mode);
                }
                0x29 | 0x25 | 0x35 | 0x2d | 0x3d | 0x39 | 0x21 | 0x31 => {
                    self.and(&opcode.mode);
                }
                0x49 | 0x45 | 0x55 | 0x4d | 0x5d | 0x59 | 0x41 | 0x51 => {
                    self.eor(&opcode.mode);
                }
                0x09 | 0x05 | 0x15 | 0x0d | 0x1d | 0x19 | 0x01 | 0x11 => {
                    self.ora(&opcode.mode);
                }
                0x0a => self.asl_accumulator(),
                0x06 | 0x16 | 0x0e | 0x1e => {
                    self.asl(&opcode.mode);
                }
                0x4a => self.lsr_accumulator(),
                0x46 | 0x56 | 0x4e | 0x5e => {
                    self.lsr(&opcode.mode);
                }
                0x2a => self.rol_accumulator(),
                0x26 | 0x36 | 0x2e | 0x3e => {
                    self.rol(&opcode.mode);
                }
                0x6a => self.ror_accumulator(),
                0x66 | 0x76 | 0x6e | 0x7e => {
                    self.ror(&opcode.mode);
                }
                0xe6 | 0xf6 | 0xee | 0xfe => {
                    self.inc(&opcode.mode);
                }
                0xc8 => self.iny(),
                0xc6 | 0xd6 | 0xce | 0xde => {
                    self.dec(&opcode.mode);
                }
                0xca => {
                    self.dex();
                }
                0x88 => {
                    self.dey();
                }
                0xc9 | 0xc5 | 0xd5 | 0xcd | 0xdd | 0xd9 | 0xc1 | 0xd1 => {
                    self.compare(&opcode.mode, self.register_a);
                }
                0xc0 | 0xc4 | 0xcc => {
                    self.compare(&opcode.mode, self.register_y);
                }
                0xe0 | 0xe4 | 0xec => self.compare(&opcode.mode, self.register_x),
                0x4c => {
                    let mem_address = self.mem_read_u16(self.program_counter);
                    self.program_counter = mem_address;
                }
                0x6c => {
                    let mem_address = self.mem_read_u16(self.program_counter);

                    let indirect_ref = if mem_address & 0x00FF == 0x00FF {
                        let lo = self.mem_read(mem_address);
                        let hi = self.mem_read(mem_address & 0xFF00);
                        (hi as u16) << 8 | (lo as u16)
                    } else {
                        self.mem_read_u16(mem_address)
                    };

                    self.program_counter = indirect_ref;
                }
                0x20 => {
                    self.stack_push_u16(self.program_counter + 2 - 1);
                    let target_address = self.mem_read_u16(self.program_counter);
                    self.program_counter = target_address
                }
                0x60 => {
                    self.program_counter = self.stack_pop_u16() + 1;
                }
                0x40 => {
                    self.status.bits = self.stack_pop();
                    self.status.remove(CpuFlags::BREAK);
                    self.status.insert(CpuFlags::BREAK2);

                    self.program_counter = self.stack_pop_u16();
                }
                0xd0 => {
                    let (taken, page_crossed) = self.branch(!self.status.contains(CpuFlags::ZERO));
                    if taken {
                        extra_cycles += 1;
                    }
                    if page_crossed {
                        extra_cycles += 1;
                    }
                }
                0x70 => {
                    let (taken, page_crossed) =
                        self.branch(self.status.contains(CpuFlags::OVERFLOW));
                    if taken {
                        extra_cycles += 1;
                    }
                    if page_crossed {
                        extra_cycles += 1;
                    }
                }
                0x50 => {
                    let (taken, page_crossed) =
                        self.branch(!self.status.contains(CpuFlags::OVERFLOW));
                    if taken {
                        extra_cycles += 1;
                    }
                    if page_crossed {
                        extra_cycles += 1;
                    }
                }
                0x10 => {
                    let (taken, page_crossed) =
                        self.branch(!self.status.contains(CpuFlags::NEGATIV));
                    if taken {
                        extra_cycles += 1;
                    }
                    if page_crossed {
                        extra_cycles += 1;
                    }
                }
                0x30 => {
                    let (taken, page_crossed) =
                        self.branch(self.status.contains(CpuFlags::NEGATIV));
                    if taken {
                        extra_cycles += 1;
                    }
                    if page_crossed {
                        extra_cycles += 1;
                    }
                }
                0xf0 => {
                    let (taken, page_crossed) = self.branch(self.status.contains(CpuFlags::ZERO));
                    if taken {
                        extra_cycles += 1;
                    }
                    if page_crossed {
                        extra_cycles += 1;
                    }
                }
                0xb0 => {
                    let (taken, page_crossed) = self.branch(self.status.contains(CpuFlags::CARRY));
                    if taken {
                        extra_cycles += 1;
                    }
                    if page_crossed {
                        extra_cycles += 1;
                    }
                }
                0x90 => {
                    let (taken, page_crossed) = self.branch(!self.status.contains(CpuFlags::CARRY));
                    if taken {
                        extra_cycles += 1;
                    }
                    if page_crossed {
                        extra_cycles += 1;
                    }
                }
                0x24 | 0x2c => {
                    self.bit(&opcode.mode);
                }
                0xa2 | 0xa6 | 0xb6 | 0xae | 0xbe => {
                    self.ldx(&opcode.mode);
                }
                0xa0 | 0xa4 | 0xb4 | 0xac | 0xbc => {
                    self.ldy(&opcode.mode);
                }
                0x86 | 0x96 | 0x8e => {
                    let addr = self.get_operand_address(&opcode.mode);
                    self.mem_write(addr, self.register_x);
                }
                0x84 | 0x94 | 0x8c => {
                    let addr = self.get_operand_address(&opcode.mode);
                    self.mem_write(addr, self.register_y);
                }
                0xa8 => {
                    self.register_y = self.register_a;
                    self.update_zero_and_negative_flags(self.register_y);
                }
                0xba => {
                    self.register_x = self.stack_pointer;
                    self.update_zero_and_negative_flags(self.register_x);
                }
                0x8a => {
                    self.register_a = self.register_x;
                    self.update_zero_and_negative_flags(self.register_a);
                }
                0x9a => {
                    self.stack_pointer = self.register_x;
                }
                0x98 => {
                    self.register_a = self.register_y;
                    self.update_zero_and_negative_flags(self.register_a);
                }
                _ => {
                    return Err(CpuError::UnsupportedOpcode {
                        opcode: code,
                        pc: opcode_pc,
                    })
                }
            }

            if program_counter_state == self.program_counter {
                self.program_counter += (opcode.len - 1) as u16;
            }

            self.cycles += opcode.cycles as u64 + extra_cycles;

            callback(self);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_0xa9_lda_immediate_load_data() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0x05, 0x00]);
        assert_eq!(cpu.register_a, 5);
        assert!(cpu.status.bits() & 0b0000_0010 == 0b00);
        assert!(cpu.status.bits() & 0b1000_0000 == 0);
    }

    #[test]
    fn test_0xa9_lda_zero_flag() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0x00, 0x00]);
        assert!(cpu.status.bits() & 0b0000_0010 == 0b10);
    }

    #[test]
    fn test_0xaa_tax_move_a_to_x() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0x0a, 0xaa, 0x00]);

        assert_eq!(cpu.register_x, 10)
    }

    #[test]
    fn test_5_ops_working_together() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0xc0, 0xaa, 0xe8, 0x00]);

        assert_eq!(cpu.register_x, 0xc1);
    }

    #[test]
    fn test_inx_overflow() {
        let mut cpu = CPU::new();
        cpu.load_and_run(vec![0xa9, 0xff, 0xaa, 0xe8, 0xe8, 0x00]);

        assert_eq!(cpu.register_x, 1);
    }

    #[test]
    fn test_lda_from_memory() {
        let mut cpu = CPU::new();
        cpu.mem_write(0x10, 0x55);

        cpu.load_and_run(vec![0xa5, 0x10, 0x00]);
        assert_eq!(cpu.register_a, 0x55);
    }

    #[test]
    fn test_ldx() {
        let mut cpu = CPU::new();
        cpu.mem_write(0x10, 0x55);

        cpu.load_and_run(vec![0xa2, 0x10, 0xa6, 0x10, 0x00]);
        assert_eq!(cpu.register_x, 0x55);
    }

    #[test]
    fn test_ldy() {
        let mut cpu = CPU::new();
        cpu.mem_write(0x10, 0x55);

        cpu.load_and_run(vec![0xa0, 0x10, 0xa4, 0x10, 0x00]);
        assert_eq!(cpu.register_y, 0x55);
    }

    #[test]
    fn test_sta() {
        let mut cpu = CPU::new();
        cpu.register_a = 0x55;
        cpu.load_and_run(vec![0xa9, 0x55, 0x85, 0x10, 0x00]);
        assert_eq!(cpu.mem_read(0x10), 0x55);
    }

    #[test]
    fn test_sta_zero_page_x() {
        let mut cpu = CPU::new();
        cpu.register_a = 0x55;
        cpu.register_x = 0x05;
        cpu.load_and_run(vec![0xa9, 0x55, 0x95, 0x05, 0x00]);
        assert_eq!(cpu.mem_read(0x05), 0x55);
    }

    #[test]
    fn test_sta_absolute() {
        let mut cpu = CPU::new();
        cpu.register_a = 0x55;
        cpu.load_and_run(vec![0xa9, 0x55, 0x8d, 0x00, 0x80, 0x00]);
        assert_eq!(cpu.mem_read(0x8000), 0x55);
    }

    #[test]
    fn test_stack_push_and_pop_u16() {
        let mut cpu = CPU::new();
        cpu.stack_push(0x55);
        cpu.stack_push(0x66);
        assert_eq!(cpu.stack_pop_u16(), 0x5566);
    }

    #[test]
    fn test_mem_read() {
        let mut cpu = CPU::new();
        cpu.memory[0x10] = 0x55;
        assert_eq!(cpu.mem_read(0x10), 0x55);
    }

    #[test]
    fn test_mem_write() {
        let mut cpu = CPU::new();
        cpu.mem_write(0x10, 0x55);
        assert_eq!(cpu.memory[0x10], 0x55);
    }

    #[test]
    fn test_mem_read_u16() {
        let mut cpu = CPU::new();
        cpu.memory[0x10] = 0x55;
        cpu.memory[0x11] = 0x66;
        assert_eq!(cpu.mem_read_u16(0x10), 0x6655);
    }

    #[test]
    fn test_mem_write_u16() {
        let mut cpu = CPU::new();
        cpu.mem_write_u16(0x10, 0x6655);
        assert_eq!(cpu.memory[0x10], 0x55);
        assert_eq!(cpu.memory[0x11], 0x66);
    }

    #[test]
    fn test_apu_register_write_is_accepted() {
        let mut cpu = CPU::new();

        cpu.mem_write(0x4000, 0xff);
        cpu.mem_write(0x4015, 0x1f);
        cpu.mem_write(0x4017, 0x7f);

        assert_eq!(cpu.mem_read(0x4000), 0x00);
        assert_eq!(cpu.mem_read(0x4015), 0x1f);
        assert_eq!(cpu.mem_read(0x4017), 0x00);
    }

    #[test]
    fn test_ppu_and_apu_ranges_are_independent() {
        let mut cpu = CPU::new();

        cpu.mem_write(0x4000, 0xaa);

        assert_eq!(cpu.mem_read(0x4000), 0x00);
        assert_ne!(cpu.memory[0x4000], 0xaa);

        cpu.mem_write(0x2000, 0x55);
        assert_eq!(cpu.mem_read(0x2000), 0x00);
        assert_eq!(cpu.memory[0x2000], 0x00);
    }

    #[test]
    fn test_trace_log_contains_registers_and_opcode() {
        let mut cpu = CPU::new();
        cpu.load(vec![0xa9, 0x05, 0xaa, 0x00]);
        cpu.reset();

        let mut logs = vec![];
        cpu.run_with_trace(|trace| logs.push(trace.to_log_line()));

        assert_eq!(logs.len(), 2);
        assert!(logs[0].contains("PC:0602 OPC:AA TAX"));
        assert!(logs[1].contains("PC:0603 OPC:00 BRK"));
        assert!(logs[0].contains("A:05"));
        assert!(logs[0].contains("X:00"));
        assert!(logs[0].contains("Y:00"));
    }

    #[test]
    fn test_trace_state_falls_back_for_unknown_opcode() {
        let mut cpu = CPU::new();
        cpu.mem_write(0x0600, 0x02);
        cpu.program_counter = 0x0600;

        let trace = cpu.capture_trace_state();

        assert_eq!(trace.mnemonic, "???");
        assert_eq!(trace.opcode, 0x02);
    }

    #[test]
    fn test_and_immediate() {
        let mut cpu = CPU::new();
        cpu.register_a = 0b1100_0000;
        cpu.load_and_run(vec![0xa9, 0b1010_1010, 0x29, 0b0101_0101, 0x00]);
        assert_eq!(cpu.register_a, 0b0000_0000);
    }

    #[test]
    fn test_eor_immediate() {
        let mut cpu = CPU::new();
        cpu.register_a = 0b1100_0000;
        cpu.load_and_run(vec![0xa9, 0b1010_1010, 0x49, 0b0101_0101, 0x00]);
        assert_eq!(cpu.register_a, 0b1111_1111);
    }

    #[test]
    fn test_ora_immediate() {
        let mut cpu = CPU::new();
        cpu.register_a = 0b1100_0000;
        cpu.load_and_run(vec![0xa9, 0b1010_1010, 0x09, 0b0101_0101, 0x00]);
        assert_eq!(cpu.register_a, 0b1111_1111);
    }

    #[test]
    fn test_asl_accumulator() {
        let mut cpu = CPU::new();
        cpu.register_a = 0b1000_0000;
        cpu.load_and_run(vec![0x0a, 0x00]);
        assert_eq!(cpu.register_a, 0b0000_0000);
        assert!(cpu.status.bits() & 0b0000_0010 == 0b10);
        assert!(cpu.status.bits() & 0b0000_0001 == 0b00);
    }

    #[test]
    fn test_asl_zero_page() {
        let mut cpu = CPU::new();
        cpu.mem_write(0x10, 0b1000_0001);
        cpu.load_and_run(vec![0x06, 0x10, 0x00]);
        assert_eq!(cpu.mem_read(0x10), 0b0000_0010);
        assert!(!cpu.status.contains(CpuFlags::ZERO));
        assert!(cpu.status.contains(CpuFlags::CARRY));
    }

    #[test]
    fn test_lsr_accumulator() {
        let mut cpu = CPU::new();
        cpu.register_a = 0b1000_0001;
        cpu.lsr_accumulator();
        assert_eq!(cpu.register_a, 0b0100_0000);
        assert!(cpu.status.contains(CpuFlags::CARRY));
        assert!(!cpu.status.contains(CpuFlags::ZERO));
        assert!(!cpu.status.contains(CpuFlags::NEGATIV));
    }

    #[test]
    fn test_rol_accumulator() {
        let mut cpu = CPU::new();
        cpu.register_a = 0b1000_0001;
        cpu.rol_accumulator();
        assert_eq!(cpu.register_a, 0b0000_0010);
        assert!(cpu.status.contains(CpuFlags::CARRY));
        assert!(!cpu.status.contains(CpuFlags::ZERO));
        assert!(!cpu.status.contains(CpuFlags::NEGATIV));
    }

    #[test]
    fn test_rol_accumulator_with_carry() {
        let mut cpu = CPU::new();
        cpu.register_a = 0b1000_0000;
        cpu.status.insert(CpuFlags::CARRY);
        cpu.rol_accumulator();
        assert_eq!(cpu.register_a, 0b0000_0001);
        assert!(cpu.status.contains(CpuFlags::CARRY));
        assert!(!cpu.status.contains(CpuFlags::ZERO));
        assert!(!cpu.status.contains(CpuFlags::NEGATIV));
    }

    #[test]
    fn test_ror_accumulator() {
        let mut cpu = CPU::new();
        cpu.register_a = 0b0000_0010;
        cpu.ror_accumulator();
        assert_eq!(cpu.register_a, 0b0000_0001);
        assert!(!cpu.status.contains(CpuFlags::CARRY));
        assert!(!cpu.status.contains(CpuFlags::ZERO));
        assert!(!cpu.status.contains(CpuFlags::NEGATIV));
    }

    #[test]
    fn test_ror_accumulator_with_carry() {
        let mut cpu = CPU::new();
        cpu.register_a = 0b0000_0001;
        cpu.status.insert(CpuFlags::CARRY);
        cpu.ror_accumulator();
        assert_eq!(cpu.register_a, 0b1000_0000);
        assert!(cpu.status.contains(CpuFlags::CARRY));
        assert!(!cpu.status.contains(CpuFlags::ZERO));
        assert!(cpu.status.contains(CpuFlags::NEGATIV));
    }

    #[test]
    fn test_dec() {
        let mut cpu = CPU::new();
        cpu.mem_write(0x10, 0x55);
        cpu.load_and_run(vec![0xc6, 0x10, 0x00]);
        assert_eq!(cpu.mem_read(0x10), 0x54);
    }

    #[test]
    fn test_bit() {
        let mut cpu = CPU::new();
        cpu.register_a = 0b1100_0000;
        cpu.mem_write(0x10, 0b1010_1010);
        cpu.load_and_run(vec![0x24, 0x10, 0x00]);
        assert!(cpu.status.contains(CpuFlags::ZERO));
        assert!(!cpu.status.contains(CpuFlags::OVERFLOW));
        assert!(cpu.status.contains(CpuFlags::NEGATIV));
    }

    #[test]
    fn test_mem_read_u16_wraps_at_top_of_address_space() {
        let mut cpu = CPU::new();
        cpu.mem_write(0xffff, 0x55);
        cpu.mem_write(0x0000, 0x66);
        assert_eq!(cpu.mem_read_u16(0xffff), 0x6655);
    }

    #[test]
    fn test_mem_write_u16_wraps_at_top_of_address_space() {
        let mut cpu = CPU::new();
        cpu.mem_write_u16(0xffff, 0x6655);
        assert_eq!(cpu.mem_read(0xffff), 0x55);
        assert_eq!(cpu.mem_read(0x0000), 0x66);
    }

    #[test]
    fn test_ppu_register_mirror_write_2008_maps_to_2000() {
        let mut cpu = CPU::new();
        cpu.mem_write(0x2008, 0x04);

        cpu.mem_write(0x2006, 0x20);
        cpu.mem_write(0x2006, 0x00);
        cpu.mem_write(0x2007, 0x11);
        cpu.mem_write(0x2007, 0x22);

        cpu.mem_write(0x2006, 0x20);
        cpu.mem_write(0x2006, 0x00);
        assert_eq!(cpu.mem_read(0x2007), 0x00);
        assert_eq!(cpu.mem_read(0x2007), 0x11);

        cpu.mem_write(0x2006, 0x20);
        cpu.mem_write(0x2006, 0x20);
        assert_eq!(cpu.mem_read(0x2007), 0x22);
        assert_eq!(cpu.mem_read(0x2007), 0x22);
    }

    #[test]
    fn test_ppu_register_mirror_access_via_3fff_maps_to_2007() {
        let mut cpu = CPU::new();

        cpu.mem_write(0x2006, 0x20);
        cpu.mem_write(0x2006, 0x00);
        cpu.mem_write(0x3fff, 0x55);

        cpu.mem_write(0x2006, 0x20);
        cpu.mem_write(0x2006, 0x00);
        assert_eq!(cpu.mem_read(0x3fff), 0x00);
        assert_eq!(cpu.mem_read(0x3fff), 0x55);
    }

    #[test]
    fn test_trigger_nmi_pushes_state_and_loads_vector() {
        let mut cpu = CPU::new();
        cpu.program_counter = 0x1234;
        cpu.status = CpuFlags::from_bits_truncate(0);
        cpu.mem_write_u16(NMI_VECTOR, 0x4567);

        cpu.trigger_nmi();

        assert_eq!(cpu.program_counter, 0x4567);
        assert!(cpu.status.contains(CpuFlags::INTERRUPT_DISABLE));
        assert_eq!(cpu.stack_pointer, STACK_RESET.wrapping_sub(3));
        assert_eq!(cpu.mem_read(0x01FD), 0x12);
        assert_eq!(cpu.mem_read(0x01FC), 0x34);
        assert_eq!(cpu.mem_read(0x01FB), CpuFlags::BREAK2.bits());
    }

    #[test]
    fn test_trigger_irq_respects_interrupt_disable_flag() {
        let mut cpu = CPU::new();
        cpu.program_counter = 0x1234;
        cpu.status = CpuFlags::INTERRUPT_DISABLE;
        cpu.mem_write_u16(IRQ_BRK_VECTOR, 0x4567);

        let handled = cpu.trigger_irq();

        assert!(!handled);
        assert_eq!(cpu.program_counter, 0x1234);
        assert_eq!(cpu.stack_pointer, STACK_RESET);
    }

    #[test]
    fn test_trigger_irq_pushes_state_and_loads_vector() {
        let mut cpu = CPU::new();
        cpu.program_counter = 0x1234;
        cpu.status = CpuFlags::from_bits_truncate(0);
        cpu.mem_write_u16(IRQ_BRK_VECTOR, 0x4567);

        let handled = cpu.trigger_irq();

        assert!(handled);
        assert_eq!(cpu.program_counter, 0x4567);
        assert!(cpu.status.contains(CpuFlags::INTERRUPT_DISABLE));
        assert_eq!(cpu.stack_pointer, STACK_RESET.wrapping_sub(3));
        assert_eq!(cpu.mem_read(0x01FD), 0x12);
        assert_eq!(cpu.mem_read(0x01FC), 0x34);
        assert_eq!(cpu.mem_read(0x01FB), CpuFlags::BREAK2.bits());
    }

    #[test]
    fn test_trigger_brk_sets_break_bit_on_stack_and_loads_vector() {
        let mut cpu = CPU::new();
        cpu.program_counter = 0x1234;
        cpu.status = CpuFlags::from_bits_truncate(0);
        cpu.mem_write_u16(IRQ_BRK_VECTOR, 0x4567);

        cpu.trigger_brk();

        assert_eq!(cpu.program_counter, 0x4567);
        assert!(cpu.status.contains(CpuFlags::INTERRUPT_DISABLE));
        assert_eq!(cpu.stack_pointer, STACK_RESET.wrapping_sub(3));
        assert_eq!(
            cpu.mem_read(0x01FB),
            (CpuFlags::BREAK | CpuFlags::BREAK2).bits()
        );
    }

    #[test]
    fn test_cycle_counting_for_simple_program() {
        let mut cpu = CPU::new();
        cpu.load(vec![0xa9, 0x01, 0xaa, 0x00]);
        cpu.reset();

        cpu.try_run_with_callback(&mut |_| {}).unwrap();

        assert_eq!(cpu.total_cycles(), 11);
    }

    #[test]
    fn test_branch_taken_adds_cycle() {
        let mut cpu = CPU::new();
        cpu.load(vec![0xa9, 0x00, 0xf0, 0x02, 0xea, 0x00]);
        cpu.reset();

        cpu.try_run_with_callback(&mut |_| {}).unwrap();

        assert_eq!(cpu.total_cycles(), 12);
    }

    #[test]
    fn test_absolute_y_page_cross_adds_cycle() {
        let mut cpu = CPU::new();
        cpu.mem_write(0x0100, 0x07);
        cpu.load(vec![0xb9, 0xff, 0x00, 0x00]);
        cpu.reset();
        cpu.register_y = 1;

        cpu.try_run_with_callback(&mut |_| {}).unwrap();

        assert_eq!(cpu.register_a, 0x07);
        assert_eq!(cpu.total_cycles(), 12);
    }
    #[test]
    fn test_try_run_reports_unsupported_opcode() {
        let mut cpu = CPU::new();
        cpu.load(vec![0x02]);
        cpu.reset();

        let err = cpu.try_run_with_callback(&mut |_| {}).unwrap_err();
        assert_eq!(
            err,
            CpuError::UnsupportedOpcode {
                opcode: 0x02,
                pc: 0x0600,
            }
        );
    }

    #[test]
    fn test_load_prg_rom_16kb_mirrors_to_upper_bank() {
        let mut cpu = CPU::new();
        let mut prg_rom = vec![0u8; 0x4000];
        prg_rom[0] = 0x11;
        prg_rom[0x3FFF] = 0x22;
        cpu.load_prg_rom(&prg_rom).unwrap();

        assert_eq!(cpu.mem_read(0x8000), 0x11);
        assert_eq!(cpu.mem_read(0xBFFF), 0x22);
        assert_eq!(cpu.mem_read(0xC000), 0x11);
        assert_eq!(cpu.mem_read(0xFFFF), 0x22);
    }

    #[test]
    fn test_load_prg_rom_32kb_maps_entire_upper_space() {
        let mut cpu = CPU::new();
        let mut prg_rom = vec![0u8; 0x8000];
        prg_rom[0] = 0x33;
        prg_rom[0x7FFF] = 0x44;
        cpu.load_prg_rom(&prg_rom).unwrap();

        assert_eq!(cpu.mem_read(0x8000), 0x33);
        assert_eq!(cpu.mem_read(0xFFFF), 0x44);
    }

    #[test]
    fn test_load_prg_rom_rejects_invalid_size() {
        let mut cpu = CPU::new();
        let err = cpu.load_prg_rom(&vec![0u8; 0x2000]).unwrap_err();
        assert_eq!(err, CpuLoadError::InvalidPrgSize(0x2000));
    }

    #[test]
    fn test_reset_vector_after_prg_load() {
        let mut cpu = CPU::new();
        let mut prg_rom = vec![0u8; 0x4000];
        // For 16KB mirrored PRG, CPU vectors at 0xFFFC map to PRG offset 0x3FFC.
        prg_rom[0x3FFC] = 0x34;
        prg_rom[0x3FFD] = 0x12;
        cpu.load_prg_rom(&prg_rom).unwrap();

        cpu.reset();

        assert_eq!(cpu.program_counter, 0x1234);
    }

    #[test]
    fn test_load_cartridge_maps_nrom_prg_and_reset_vector() {
        let mut cpu = CPU::new();
        let mut prg_rom = vec![0u8; 0x4000];
        prg_rom[0] = 0x11;
        prg_rom[0x3FFF] = 0x22;
        prg_rom[0x3FFC] = 0x78;
        prg_rom[0x3FFD] = 0x56;

        let rom = Rom {
            prg_rom,
            chr_rom: vec![0; 0x2000],
            mapper: 0,
            mirroring: Mirroring::Horizontal,
            has_chr_ram: false,
        };

        cpu.load_cartridge(rom).unwrap();
        assert_eq!(cpu.mem_read(0x8000), 0x11);
        assert_eq!(cpu.mem_read(0xC000), 0x11);
        assert_eq!(cpu.mem_read(0xFFFF), 0x22);

        cpu.reset();
        assert_eq!(cpu.program_counter, 0x5678);
    }

    #[test]
    fn test_load_cartridge_rejects_unsupported_mapper() {
        let mut cpu = CPU::new();
        let rom = Rom {
            prg_rom: vec![0; 0x4000],
            chr_rom: vec![0; 0x2000],
            mapper: 1,
            mirroring: Mirroring::Horizontal,
            has_chr_ram: false,
        };

        let err = cpu.load_cartridge(rom).unwrap_err();
        assert_eq!(err, CpuLoadError::UnsupportedMapper(1));
    }
}
