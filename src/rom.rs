const INES_HEADER_SIZE: usize = 16;
const INES_TRAINER_SIZE: usize = 512;
const PRG_ROM_PAGE_SIZE: usize = 16 * 1024;
const CHR_ROM_PAGE_SIZE: usize = 8 * 1024;
const INES_MAGIC: [u8; 4] = [0x4E, 0x45, 0x53, 0x1A];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mirroring {
    Horizontal,
    Vertical,
    FourScreen,
}

#[derive(Debug, PartialEq, Eq)]
pub enum RomError {
    InvalidHeader,
    UnsupportedMapper(u8),
    Truncated,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Rom {
    pub prg_rom: Vec<u8>,
    pub chr_rom: Vec<u8>,
    pub mapper: u8,
    pub mirroring: Mirroring,
}

impl Rom {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, RomError> {
        if raw.len() < INES_HEADER_SIZE {
            return Err(RomError::Truncated);
        }

        if raw[0..4] != INES_MAGIC {
            return Err(RomError::InvalidHeader);
        }

        let prg_rom_banks = raw[4] as usize;
        let chr_rom_banks = raw[5] as usize;
        let flags6 = raw[6];
        let flags7 = raw[7];
        let mapper = (flags6 >> 4) | (flags7 & 0xF0);

        if mapper != 0 {
            return Err(RomError::UnsupportedMapper(mapper));
        }

        let trainer_present = flags6 & 0b0000_0100 != 0;
        let mirroring = if flags6 & 0b0000_1000 != 0 {
            Mirroring::FourScreen
        } else if flags6 & 0b0000_0001 != 0 {
            Mirroring::Vertical
        } else {
            Mirroring::Horizontal
        };

        let mut cursor = INES_HEADER_SIZE;
        if trainer_present {
            cursor += INES_TRAINER_SIZE;
        }

        let prg_size = prg_rom_banks * PRG_ROM_PAGE_SIZE;
        let chr_size = chr_rom_banks * CHR_ROM_PAGE_SIZE;
        let required_size = cursor + prg_size + chr_size;
        if raw.len() < required_size {
            return Err(RomError::Truncated);
        }

        let prg_rom = raw[cursor..cursor + prg_size].to_vec();
        cursor += prg_size;
        let chr_rom = raw[cursor..cursor + chr_size].to_vec();

        Ok(Rom {
            prg_rom,
            chr_rom,
            mapper,
            mirroring,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn build_ines(prg_banks: u8, chr_banks: u8, flags6: u8, flags7: u8, payload: Vec<u8>) -> Vec<u8> {
        let mut bytes = vec![0u8; INES_HEADER_SIZE];
        bytes[0..4].copy_from_slice(&INES_MAGIC);
        bytes[4] = prg_banks;
        bytes[5] = chr_banks;
        bytes[6] = flags6;
        bytes[7] = flags7;
        bytes.extend_from_slice(&payload);
        bytes
    }

    #[test]
    fn test_from_bytes_reads_prg_and_chr() {
        let prg = vec![0xAA; PRG_ROM_PAGE_SIZE];
        let chr = vec![0xBB; CHR_ROM_PAGE_SIZE];
        let raw = build_ines(1, 1, 0, 0, [prg.clone(), chr.clone()].concat());

        let rom = Rom::from_bytes(&raw).unwrap();
        assert_eq!(rom.mapper, 0);
        assert_eq!(rom.mirroring, Mirroring::Horizontal);
        assert_eq!(rom.prg_rom, prg);
        assert_eq!(rom.chr_rom, chr);
    }

    #[test]
    fn test_from_bytes_skips_trainer() {
        let trainer = vec![0xCC; INES_TRAINER_SIZE];
        let prg = vec![0xAA; PRG_ROM_PAGE_SIZE];
        let raw = build_ines(1, 0, 0b0000_0100, 0, [trainer, prg.clone()].concat());

        let rom = Rom::from_bytes(&raw).unwrap();
        assert_eq!(rom.prg_rom, prg);
        assert!(rom.chr_rom.is_empty());
    }

    #[test]
    fn test_from_bytes_rejects_invalid_header() {
        let mut raw = vec![0u8; INES_HEADER_SIZE];
        raw[0..4].copy_from_slice(b"BAD!");
        assert_eq!(Rom::from_bytes(&raw), Err(RomError::InvalidHeader));
    }

    #[test]
    fn test_from_bytes_rejects_non_nrom_mapper() {
        let prg = vec![0xAA; PRG_ROM_PAGE_SIZE];
        let raw = build_ines(1, 0, 0b0001_0000, 0, prg);
        assert_eq!(Rom::from_bytes(&raw), Err(RomError::UnsupportedMapper(1)));
    }
}
