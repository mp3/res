const PRG_ROM_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

pub trait Mapper {
    fn cpu_read(&self, addr: u16) -> Option<u8>;
    fn cpu_write(&mut self, addr: u16, data: u8) -> bool;
    fn ppu_read(&self, addr: u16) -> Option<u8>;
    fn ppu_write(&mut self, addr: u16, data: u8) -> bool;
}

#[derive(Debug, PartialEq, Eq)]
pub enum MapperError {
    InvalidPrgSize(usize),
}

pub struct NromMapper {
    prg_rom: Vec<u8>,
    chr: Vec<u8>,
    chr_is_ram: bool,
}

impl NromMapper {
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, has_chr_ram: bool) -> Result<Self, MapperError> {
        match prg_rom.len() {
            0x4000 | 0x8000 => {}
            size => return Err(MapperError::InvalidPrgSize(size)),
        }

        let (chr, chr_is_ram) = if has_chr_ram {
            (vec![0; CHR_BANK_SIZE], true)
        } else {
            (chr_rom, false)
        };

        Ok(Self {
            prg_rom,
            chr,
            chr_is_ram,
        })
    }
}

impl Mapper for NromMapper {
    fn cpu_read(&self, addr: u16) -> Option<u8> {
        if !(0x8000..=0xFFFF).contains(&addr) {
            return None;
        }

        let mapped = if self.prg_rom.len() == PRG_ROM_BANK_SIZE {
            ((addr - 0x8000) as usize) % PRG_ROM_BANK_SIZE
        } else {
            (addr - 0x8000) as usize
        };
        Some(self.prg_rom[mapped])
    }

    fn cpu_write(&mut self, addr: u16, _data: u8) -> bool {
        (0x8000..=0xFFFF).contains(&addr)
    }

    fn ppu_read(&self, addr: u16) -> Option<u8> {
        if addr > 0x1FFF {
            return None;
        }

        if self.chr.is_empty() {
            return Some(0);
        }

        Some(self.chr[addr as usize])
    }

    fn ppu_write(&mut self, addr: u16, data: u8) -> bool {
        if addr > 0x1FFF {
            return false;
        }

        if self.chr_is_ram {
            self.chr[addr as usize] = data;
            true
        } else {
            true
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_nrom_128_cpu_mirrors_upper_bank() {
        let mut prg = vec![0u8; 0x4000];
        prg[0] = 0x11;
        prg[0x3FFF] = 0x22;
        let mapper = NromMapper::new(prg, vec![0; CHR_BANK_SIZE], false).unwrap();

        assert_eq!(mapper.cpu_read(0x8000), Some(0x11));
        assert_eq!(mapper.cpu_read(0xBFFF), Some(0x22));
        assert_eq!(mapper.cpu_read(0xC000), Some(0x11));
        assert_eq!(mapper.cpu_read(0xFFFF), Some(0x22));
    }

    #[test]
    fn test_nrom_256_cpu_uses_full_32kb_prg() {
        let mut prg = vec![0u8; 0x8000];
        prg[0] = 0x33;
        prg[0x7FFF] = 0x44;
        let mapper = NromMapper::new(prg, vec![0; CHR_BANK_SIZE], false).unwrap();

        assert_eq!(mapper.cpu_read(0x8000), Some(0x33));
        assert_eq!(mapper.cpu_read(0xFFFF), Some(0x44));
    }

    #[test]
    fn test_nrom_chr_rom_ignores_writes() {
        let mapper = NromMapper::new(vec![0; 0x4000], vec![0xAB; CHR_BANK_SIZE], false).unwrap();
        let mut mapper = mapper;

        assert_eq!(mapper.ppu_read(0x0010), Some(0xAB));
        assert!(mapper.ppu_write(0x0010, 0xCD));
        assert_eq!(mapper.ppu_read(0x0010), Some(0xAB));
    }

    #[test]
    fn test_nrom_chr_ram_stores_written_values() {
        let mut mapper = NromMapper::new(vec![0; 0x4000], vec![], true).unwrap();

        assert_eq!(mapper.ppu_read(0x0010), Some(0x00));
        assert!(mapper.ppu_write(0x0010, 0xCD));
        assert_eq!(mapper.ppu_read(0x0010), Some(0xCD));
    }
}
