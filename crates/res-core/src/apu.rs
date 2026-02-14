pub struct Apu {
    registers: [u8; 0x18],
}

impl Apu {
    pub fn new() -> Self {
        Self {
            registers: [0; 0x18],
        }
    }

    fn is_apu_register(addr: u16) -> bool {
        (0x4000..=0x4017).contains(&addr)
    }

    pub fn write_register(&mut self, addr: u16, data: u8) {
        if Self::is_apu_register(addr) {
            self.registers[(addr - 0x4000) as usize] = data;
        }
    }

    pub fn read_register(&self, addr: u16) -> u8 {
        if !Self::is_apu_register(addr) {
            return 0;
        }

        // APU is currently a stub. Most registers are treated as write-only and
        // return `0` on reads, but `$4015` (status) is surfaced so callers can
        // verify register wiring while full audio emulation is pending.
        if addr == 0x4015 {
            return self.registers[(addr - 0x4000) as usize];
        }

        0
    }
}

#[cfg(test)]
mod test {
    use super::Apu;

    #[test]
    fn test_apu_write_and_read_paths_are_stubbed() {
        let mut apu = Apu::new();

        apu.write_register(0x4000, 0xFF);
        apu.write_register(0x4017, 0x80);

        assert_eq!(apu.read_register(0x4000), 0x00);
        assert_eq!(apu.read_register(0x4017), 0x00);
    }

    #[test]
    fn test_apu_status_register_readback_is_available_in_stub() {
        let mut apu = Apu::new();

        apu.write_register(0x4015, 0x1F);

        assert_eq!(apu.read_register(0x4015), 0x1F);
    }
}
