use std::collections::HashMap;
use crate::opcodes;

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

pub struct CPU {
  pub register_a: u8,
  pub register_x: u8,
  pub register_y: u8,
  pub status: CpuFlags,
  pub program_counter: u16,
  pub stack_pointer: u8,
  memory: [u8; 0xffff],
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

trait Mem {
  fn mem_read(&self, addr: u16) -> u8;

  fn mem_write(&mut self, addr: u16, data: u8);

  fn mem_read_u16(&mut self, pos: u16) -> u16 {
    let lo = self.mem_read(pos) as u16;
    let hi = self.mem_read(pos + 1) as u16;
    (hi << 8) | (lo as u16)
  }

  fn mem_write_u16(&mut self, pos: u16, data: u16) {
    let hi = (data >> 8) as u8;
    let lo = (data & 0xff) as u8;
    self.mem_write(pos, lo);
    self.mem_write(pos + 1, hi);
  }
}

impl Mem for CPU {
  fn mem_read(&self, addr: u16) -> u8 {
    self.memory[addr as usize]
  }

  fn mem_write(&mut self, addr: u16, data: u8) {
    self.memory[addr as usize] = data;
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
      memory: [0; 0xffff],
    }
  }

  fn get_operand_address(&mut self, mode: &AddressingMode) -> u16 {
    match mode {
      AddressingMode::Immediate => self.program_counter,
      AddressingMode::ZeroPage => self.mem_read(self.program_counter) as u16,
      AddressingMode::Absolute => self.mem_read_u16(self.program_counter),
      AddressingMode::ZeroPage_X => {
        let pos = self.mem_read(self.program_counter);
        let addr = pos.wrapping_add(self.register_x) as u16;
        addr
      }
      AddressingMode::ZeroPage_Y => {
        let pos = self.mem_read(self.program_counter);
        let addr = pos.wrapping_add(self.register_y) as u16;
        addr
      }
      AddressingMode::Absolute_X => {
        let base = self.mem_read_u16(self.program_counter);
        let addr = base.wrapping_add(self.register_x as u16);
        addr
      }
      AddressingMode::Absolute_Y => {
        let base = self.mem_read_u16(self.program_counter);
        let addr = base.wrapping_add(self.register_y as u16);
        addr
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
        let deref = deref_base.wrapping_add(self.register_y as u16);
        deref
      }
      AddressingMode::NoneAddressing => {
        panic!("mode {:?} is not supported", mode)
      }
    }
  }

  fn lda(&mut self, mode: &AddressingMode) {
    let addr = self.get_operand_address(mode);
    let value = self.mem_read(addr);

    self.register_a = value;
    self.update_zero_and_negative_flags(self.register_a);
  }

  fn tax(&mut self) {
    self.register_x = self.register_a;
    self.update_zero_and_negative_flags(self.register_x);
  }

  fn inx(&mut self) {
    self.register_x = self.register_x.wrapping_add(1);
    self.update_zero_and_negative_flags(self.register_x);
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
    let addr = self.get_operand_address(mode);
    let data = self.mem_read(addr);
    self.add_to_refister_a(((data as i8).wrapping_neg().wrapping_sub(1)) as u8);
  }

  fn adc(&mut self, mode: &AddressingMode) {
    let addr = self.get_operand_address(mode);
    let value = self.mem_read(addr);
    self.add_to_refister_a(value);
  }

  fn asl_accumulator(&mut self) {
    let mut data = self.register_a;
    if data >> 7 == 1 {
      self.set_carry_flag();
    } else {
      self.clear_carry_flag();
    }
    data = data << 1;
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
    data = data << 1;
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
    data = data >> 1;
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
    data = data >> 1;
    self.mem_write(addr, data);
    self.update_zero_and_negative_flags(data);
    data
  }

  fn rol(&mut self, mode: &AddressingMode) -> u8 {
    let addr = self.get_operand_address(mode);
    let mut data = self.mem_read(addr);
    let old_canary = self.status.contains(CpuFlags::CARRY);

    if data >> 7 == 1 {
      self.set_carry_flag();
    } else {
      self.clear_carry_flag();
    }

    data = data << 1;

    if old_canary {
      data = data | 1;
    }
    self.mem_write(addr, data);
    self.update_zero_and_negative_flags(data);
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

    data = data << 1;
    if old_carry {
      data = data | 1;
    }
    self.set_register_a(data);
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

  fn add_to_refister_a(&mut self, data: u8) {
    let sum = self.register_a as u16 + data as u16 + (
      if self.status.contains(CpuFlags::CARRY) {
        1
      } else {
        0
      }
    ) as u16;

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

    if result & 0b1000_0000 != 0 {
      self.status.insert(CpuFlags::NEGATIV);
    } else {
      self.status.remove(CpuFlags::NEGATIV);
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

  fn stack_pop_u16(&mut self) -> u16 {
    let lo = self.stack_pop() as u16;
    let hi = self.stack_pop() as u16;

    hi << 8 | lo
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
  
  pub fn load(&mut self, program: Vec<u8>) {
    self.memory[0x8000 .. (0x8000 + program.len())].copy_from_slice(&program[..]);
    self.mem_write_u16(0xFFFC, 0x8000);
  }

  pub fn run(&mut self) {
    let ref opcodes: HashMap<u8, &'static opcodes::OpCode> = *opcodes::OPCODES_MAP;

    loop {
      let code = self.mem_read(self.program_counter);
      self.program_counter += 1;
      let program_counter_state = self.program_counter;

      let opcode = opcodes.get(&code).expect(&format!("Opcode {:x} is not recognized", code));

      match code {
        0xa9 | 0xa5 | 0xb5 | 0xad | 0xbd | 0xb9 | 0xa1 | 0xb1 => {
          self.lda(&opcode.mode);
        }

        0x85 | 0x95 | 0x8d | 0x9d | 0x99 | 0x81 | 0x91 => {
          self.sta(&opcode.mode);
        }

        0x18 => self.clear_carry_flag(),
        0x38 => self.set_carry_flag(),

        0xAA => self.tax(),
        0xE8 => self.inx(),
        0x00 => return,
        0x08 => {
          self.php();
        },
        0x28 => {
          self.plp();
        },
        0xea => {
          // do nothing
        },
        0x40 => {
          self.status.bits = self.stack_pop();
          self.status.remove(CpuFlags::BREAK);
          self.status.insert(CpuFlags::BREAK2);
          
          self.program_counter = self.stack_pop_u16();
        },
        0x69 | 0x65 | 0x75 | 0x6d | 0x7d | 0x79 | 0x61 | 0x71 => {
          self.adc(&opcode.mode);
        },
        0xe9 | 0xe5 | 0xf5 | 0xed | 0xfd | 0xf9 | 0xe1 | 0xf1 => {
          self.sbc(&opcode.mode);
        },
        0x29 | 0x25 | 0x35 | 0x2d | 0x3d | 0x39 | 0x21 | 0x31 => {
          self.and(&opcode.mode);
        },
        0x49 | 0x45 | 0x55 | 0x4d | 0x5d | 0x59 | 0x41 | 0x51 => {
          self.eor(&opcode.mode);
        },
        0x09 | 0x05 | 0x15 | 0x0d | 0x1d | 0x19 | 0x01 | 0x11 => {
          self.ora(&opcode.mode);
        },
        0x0a => self.asl_accumulator(),
        0x06 | 0x16 | 0x0e | 0x1e => {
          self.asl(&opcode.mode);
        },
        0x4a => self.lsr_accumulator(),
        0x46 | 0x56 | 0x4e | 0x5e => {
          self.lsr(&opcode.mode);
        },
        0x2a => self.rol_accumulator(),
        0x26 | 0x36 | 0x2e | 0x3e => {
          self.rol(&opcode.mode);
        },
        _ => todo!(),
      }

      if program_counter_state == self.program_counter {
        self.program_counter += (opcode.len - 1) as u16;
      }
    }
  }

  pub fn reset(&mut self) {
    self.register_a = 0;
    self.register_x = 0;
    self.register_y = 0;
    self.stack_pointer = STACK_RESET;
    self.status = CpuFlags::from_bits_truncate(0b100100);

    self.program_counter = self.mem_read_u16(0xFFFC);
  }

  pub fn load_and_run(&mut self, program: Vec<u8>) {
    self.load(program);
    self.reset();
    self.run();
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
    cpu.load_and_run(vec![0xa9, 0xff, 0xaa,0xe8, 0xe8, 0x00]);

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
  fn test_sta_absolute_x() {
    let mut cpu = CPU::new();
    cpu.register_a = 0x55;
    cpu.register_x = 0x01;
    cpu.load_and_run(vec![0xa9, 0x55, 0x9d, 0x00, 0x80, 0x00]);
    assert_eq!(cpu.mem_read(0x8001), 0x55);
  }

  #[test]
  fn test_sta_absolute_y() {
    let mut cpu = CPU::new();
    cpu.register_a = 0x55;
    cpu.register_y = 0x01;
    cpu.load_and_run(vec![0xa9, 0x55, 0x99, 0x00, 0x80, 0x00]);
    assert_eq!(cpu.mem_read(0x8001), 0x55);
  }

  #[test]
  fn test_stack_push_and_pop() {
    let mut cpu = CPU::new();
    cpu.stack_push(0x55);
    assert_eq!(cpu.stack_pop(), 0x55);
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
    assert!(cpu.status.contains(CpuFlags::ZERO) == false);
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

  // #[test]
  // fn test_sta_indirect_x() {
  //   let mut cpu = CPU::new();
  //   cpu.register_a = 0x55;
  //   cpu.register_x = 0x01;
  //   cpu.mem_write(0x01, 0x05);
  //   cpu.mem_write(0x06, 0x80);
  //   cpu.load_and_run(vec![0xa9, 0x55, 0x81, 0x00, 0x00]);
  //   assert_eq!(cpu.mem_read(0x8005), 0x55);
  // }

  // #[test]
  // fn test_sta_indirect_y() {
  //   let mut cpu = CPU::new();
  //   cpu.register_a = 0x55;
  //   cpu.register_y = 0x01;
  //   cpu.mem_write(0x01, 0x05);
  //   cpu.mem_write(0x06, 0x80);
  //   cpu.load_and_run(vec![0xa9, 0x55, 0x91, 0x00, 0x00]);
  //   assert_eq!(cpu.mem_read(0x8006), 0x55);
  // }

  // #[test]
  // fn test_adc() {
  //   let mut cpu = CPU::new();
  //   cpu.register_a = 0x55;
  //   cpu.load_and_run(vec![0xa9, 0x01, 0x69, 0x01, 0x00]);
  //   assert_eq!(cpu.register_a, 0x56);
  // }

  // #[test]
  // fn test_adc_overflow() {
  //   let mut cpu = CPU::new();
  //   cpu.register_a = 0x7f;
  //   cpu.load_and_run(vec![0xa9, 0x01, 0x69, 0x01, 0x00]);
  //   assert_eq!(cpu.register_a, 0x80);
  //   assert!(cpu.status.bits() & 0b0100_0000 == 0b0100_0000);
  // }

  // #[test]
  // fn test_sbc() {
  //   let mut cpu = CPU::new();
  //   cpu.register_a = 0x55;
  //   cpu.load_and_run(vec![0xa9, 0x01, 0xe9, 0x01, 0x00]);
  //   assert_eq!(cpu.register_a, 0x54);
  // }

  // #[test]
  // fn test_sbc_overflow() {
  //   let mut cpu = CPU::new();
  //   cpu.register_a = 0x80;
  //   cpu.load_and_run(vec![0xa9, 0x01, 0xe9, 0x01, 0x00]);
  //   assert_eq!(cpu.register_a, 0x7f);
  //   assert!(cpu.status.bits() & 0b0100_0000 == 0);
  // }

  // #[test]
  // fn test_php() {
  //   let mut cpu = CPU::new();
  //   cpu.status.bits = 0b1100_0000;
  //   cpu.php();
  //   assert_eq!(cpu.stack_pop(), 0b1100_0010);
  // }

  // #[test]
  // fn test_plp() {
  //   let mut cpu = CPU::new();
  //   cpu.stack_push(0b1100_0010);
  //   cpu.plp();
  //   assert_eq!(cpu.status.bits, 0b1100_0000);
  // }

  // #[test]
  // fn test_reset() {
  //   let mut cpu = CPU::new();
  //   cpu.register_a = 0x55;
  //   cpu.register_x = 0x55;
  //   cpu.register_y = 0x55;
  //   cpu.status.bits = 0x55;
  //   cpu.stack_pointer = 0x55;
  //   cpu.program_counter = 0x55;
  //   cpu.reset();
  //   assert_eq!(cpu.register_a, 0);
  //   assert_eq!(cpu.register_x, 0);
  //   assert_eq!(cpu.register_y, 0);
  //   assert_eq!(cpu.status.bits, 0b100100);
  //   assert_eq!(cpu.stack_pointer, STACK_RESET);
  //   assert_eq!(cpu.program_counter, 0x8000);
  // }

  // #[test]
  // fn test_run() {
  //   let mut cpu = CPU::new();
  //   cpu.load(vec![0xa9, 0x01, 0x00]);
  //   cpu.run();
  //   assert_eq!(cpu.register_a, 0x01);
  // }

  // #[test]
  // fn test_load_and_run() {
  //   let mut cpu = CPU::new();
  //   cpu.load_and_run(vec![0xa9, 0x01, 0x00]);
  //   assert_eq!(cpu.register_a, 0x01);
  // }

  // #[test]
  // fn test_get_operand_address() {
  //   let mut cpu = CPU::new();
  //   cpu.register_x = 0x01;
  //   cpu.mem_write(0x01, 0x05);
  //   cpu.mem_write(0x06, 0x80);
  //   assert_eq!(cpu.get_operand_address(&AddressingMode::Indirect_X), 0x8005);
  // }

  // #[test]
  // fn test_update_zero_and_negative_flags() {
  //   let mut cpu = CPU::new();
  //   cpu.update_zero_and_negative_flags(0);
  //   assert_eq!(cpu.status.bits, 0b0000_0010);
  //   cpu.update_zero_and_negative_flags(0x80);
  //   assert_eq!(cpu.status.bits, 0b1000_0000);
  // }

  // #[test]
  // fn test_add_to_refister_a() {
  //   let mut cpu = CPU::new();
  //   cpu.add_to_refister_a(0x01);
  //   assert_eq!(cpu.register_a, 0x01);
  //   cpu.add_to_refister_a(0xff);
  //   assert_eq!(cpu.register_a, 0x00);
  //   assert_eq!(cpu.status.bits, 0b0000_0011);
  // }

  // #[test]
  // fn test_set_register_a() {
  //   let mut cpu = CPU::new();
  //   cpu.set_register_a(0x01);
  //   assert_eq!(cpu.register_a, 0x01);
  //   assert_eq!(cpu.status.bits, 0b0000_0000);
  //   cpu.set_register_a(0x00);
  //   assert_eq!(cpu.status.bits, 0b0000_0010);
  // }
}
