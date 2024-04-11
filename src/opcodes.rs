use crate::cpu::AddressingMode;
use std::collections::HashMap;

pub struct Opcode {
  pub code: u8,
  pub mnemonic: &'static str,
  pub len: u8,
  pub cycles: u8,
  pub mode: AddressingMode,
}

impl Opcode {
  fn new(code: u8, mnemonic: &'static str, len: u8, cycles: u8, mode: AddressingMode) -> Self {
    Opcode {
      code,
      mnemonic,
      len,
      cycles,
      mode,
    }
  }
}

lazy_static! {
  pub static ref CPU_OPS_CODES: Vec<Opcode> = vec![
    Opcode::new(0x00, "BRK", 1, 7, AddressingMode::NoneAddressing),
    Opcode::new(0xaa, "TAX", 1, 2, AddressingMode::NoneAddressing),
    Opcode::new(0xe8, "INX", 1, 2, AddressingMode::NoneAddressing),

    Opcode::new(0xa9, "LDA", 2, 2, AddressingMode::Immediate),
    Opcode::new(0xa5, "LDA", 2, 3, AddressingMode::ZeroPage),
    Opcode::new(0xb5, "LDA", 2, 4, AddressingMode::ZeroPage_X),
    Opcode::new(0xad, "LDA", 3, 4, AddressingMode::Absolute),
    Opcode::new(0xbd, "LDA", 3, 4, AddressingMode::Absolute_X),
    Opcode::new(0xb9, "LDA", 3, 4, AddressingMode::Absolute_Y),
    Opcode::new(0xa1, "LDA", 2, 6, AddressingMode::Indirect_X),
    Opcode::new(0xb1, "LDA", 2, 5, AddressingMode::Indirect_Y),

    Opcode::new(0x85, "STA", 2, 3, AddressingMode::ZeroPage),
    Opcode::new(0x95, "STA", 2, 4, AddressingMode::ZeroPage_X),
    Opcode::new(0x8d, "STA", 3, 4, AddressingMode::Absolute),
    Opcode::new(0x9d, "STA", 3, 5, AddressingMode::Absolute_X),
    Opcode::new(0x99, "STA", 3, 5, AddressingMode::Absolute_Y),
    Opcode::new(0x81, "STA", 2, 6, AddressingMode::Indirect_X),
    Opcode::new(0x91, "STA", 2, 6, AddressingMode::Indirect_Y),
  ];

  pub static ref OPCODES_MAP: HashMap<u8, &'static Opcode> = {
    let mut map = HashMap::new();
    for cpuop in &*CPU_OPS_CODES {
      map.insert(cpuop.code, cpuop);
    }
    map
  };
}
