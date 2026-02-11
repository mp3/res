use crate::rom::Mirroring;

const PPU_CTRL: u16 = 0x2000;
const PPU_MASK: u16 = 0x2001;
const PPU_STATUS: u16 = 0x2002;
const PPU_OAM_ADDR: u16 = 0x2003;
const PPU_OAM_DATA: u16 = 0x2004;
const PPU_SCROLL: u16 = 0x2005;
const PPU_ADDR: u16 = 0x2006;
const PPU_DATA: u16 = 0x2007;

pub struct Ppu {
    ctrl: u8,
    mask: u8,
    status: u8,
    oam_addr: u8,
    addr_latch: bool,
    scroll_latch: bool,
    vram_addr: u16,
    temp_vram_addr: u16,
    read_buffer: u8,
    vram: [u8; 2048],
    palette_table: [u8; 32],
    oam_data: [u8; 256],
    mirroring: Mirroring,
}

impl Ppu {
    pub fn new(mirroring: Mirroring) -> Self {
        Self {
            ctrl: 0,
            mask: 0,
            status: 0,
            oam_addr: 0,
            addr_latch: false,
            scroll_latch: false,
            vram_addr: 0,
            temp_vram_addr: 0,
            read_buffer: 0,
            vram: [0; 2048],
            palette_table: [0; 32],
            oam_data: [0; 256],
            mirroring,
        }
    }

    pub fn set_mirroring(&mut self, mirroring: Mirroring) {
        self.mirroring = mirroring;
    }

    pub fn read_register(&mut self, reg: u16) -> u8 {
        match reg {
            PPU_CTRL | PPU_MASK | PPU_OAM_ADDR | PPU_SCROLL | PPU_ADDR => 0,
            PPU_STATUS => {
                let status = self.status;
                self.status &= 0x7F;
                self.addr_latch = false;
                self.scroll_latch = false;
                status
            }
            PPU_OAM_DATA => self.oam_data[self.oam_addr as usize],
            PPU_DATA => self.read_ppu_data(),
            _ => 0,
        }
    }

    pub fn write_register(&mut self, reg: u16, data: u8) {
        match reg {
            PPU_CTRL => {
                self.ctrl = data;
                self.temp_vram_addr =
                    (self.temp_vram_addr & !0x0C00) | (((data as u16) & 0x03) << 10);
            }
            PPU_MASK => self.mask = data,
            PPU_STATUS => {}
            PPU_OAM_ADDR => self.oam_addr = data,
            PPU_OAM_DATA => {
                self.oam_data[self.oam_addr as usize] = data;
                self.oam_addr = self.oam_addr.wrapping_add(1);
            }
            PPU_SCROLL => {
                if !self.scroll_latch {
                    self.temp_vram_addr = (self.temp_vram_addr & !0x001F) | ((data as u16) >> 3);
                    self.scroll_latch = true;
                } else {
                    self.temp_vram_addr = (self.temp_vram_addr & !0x73E0)
                        | (((data as u16) & 0x07) << 12)
                        | (((data as u16) & 0xF8) << 2);
                    self.scroll_latch = false;
                }
            }
            PPU_ADDR => {
                if !self.addr_latch {
                    self.temp_vram_addr =
                        (self.temp_vram_addr & 0x00FF) | (((data as u16) & 0x3F) << 8);
                    self.addr_latch = true;
                } else {
                    self.temp_vram_addr = (self.temp_vram_addr & 0xFF00) | data as u16;
                    self.vram_addr = self.temp_vram_addr;
                    self.addr_latch = false;
                }
            }
            PPU_DATA => self.write_ppu_data(data),
            _ => {}
        }
    }

    fn vram_addr_increment(&self) -> u16 {
        if self.ctrl & 0x04 != 0 {
            32
        } else {
            1
        }
    }

    fn read_ppu_data(&mut self) -> u8 {
        let addr = self.normalize_ppu_addr(self.vram_addr);
        let result = if addr >= 0x3F00 {
            let value = self.ppu_mem_read(addr);
            self.read_buffer = self.ppu_mem_read(addr.wrapping_sub(0x1000));
            value
        } else {
            let buffered = self.read_buffer;
            self.read_buffer = self.ppu_mem_read(addr);
            buffered
        };

        self.vram_addr = self.vram_addr.wrapping_add(self.vram_addr_increment());
        result
    }

    fn write_ppu_data(&mut self, data: u8) {
        let addr = self.normalize_ppu_addr(self.vram_addr);
        self.ppu_mem_write(addr, data);
        self.vram_addr = self.vram_addr.wrapping_add(self.vram_addr_increment());
    }

    fn normalize_ppu_addr(&self, addr: u16) -> u16 {
        addr & 0x3FFF
    }

    fn ppu_mem_read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => 0,
            0x2000..=0x2FFF => {
                let idx = self.mirror_vram_addr(addr);
                self.vram[idx]
            }
            0x3000..=0x3EFF => {
                let mirrored = addr - 0x1000;
                let idx = self.mirror_vram_addr(mirrored);
                self.vram[idx]
            }
            0x3F00..=0x3FFF => {
                let idx = self.mirror_palette_addr(addr);
                self.palette_table[idx]
            }
            _ => 0,
        }
    }

    fn ppu_mem_write(&mut self, addr: u16, data: u8) {
        match addr {
            0x0000..=0x1FFF => {}
            0x2000..=0x2FFF => {
                let idx = self.mirror_vram_addr(addr);
                self.vram[idx] = data;
            }
            0x3000..=0x3EFF => {
                let mirrored = addr - 0x1000;
                let idx = self.mirror_vram_addr(mirrored);
                self.vram[idx] = data;
            }
            0x3F00..=0x3FFF => {
                let idx = self.mirror_palette_addr(addr);
                self.palette_table[idx] = data;
            }
            _ => {}
        }
    }

    fn mirror_vram_addr(&self, addr: u16) -> usize {
        let vram_index = (addr - 0x2000) as usize;
        let table = vram_index / 0x400;
        let offset = vram_index % 0x400;

        let mapped_table = match self.mirroring {
            Mirroring::Vertical => match table {
                0 | 2 => 0,
                1 | 3 => 1,
                _ => unreachable!(),
            },
            Mirroring::Horizontal => match table {
                0 | 1 => 0,
                2 | 3 => 1,
                _ => unreachable!(),
            },
            Mirroring::FourScreen => {
                // TODO: Implement dedicated 4-screen nametable memory.
                match table {
                    0 | 2 => 0,
                    1 | 3 => 1,
                    _ => unreachable!(),
                }
            }
        };

        mapped_table * 0x400 + offset
    }

    fn mirror_palette_addr(&self, addr: u16) -> usize {
        let mut idx = ((addr - 0x3F00) % 0x20) as usize;
        if matches!(idx, 0x10 | 0x14 | 0x18 | 0x1C) {
            idx -= 0x10;
        }
        idx
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn set_ppu_addr(ppu: &mut Ppu, addr: u16) {
        ppu.write_register(PPU_ADDR, (addr >> 8) as u8);
        ppu.write_register(PPU_ADDR, (addr & 0xFF) as u8);
    }

    #[test]
    fn test_ppuaddr_and_ppudata_round_trip() {
        let mut ppu = Ppu::new(Mirroring::Horizontal);

        set_ppu_addr(&mut ppu, 0x2000);
        ppu.write_register(PPU_DATA, 0x12);

        set_ppu_addr(&mut ppu, 0x2000);
        assert_eq!(ppu.read_register(PPU_DATA), 0x00);
        assert_eq!(ppu.read_register(PPU_DATA), 0x12);
    }

    #[test]
    fn test_ppuctrl_bit2_changes_ppudata_increment() {
        let mut ppu = Ppu::new(Mirroring::Horizontal);

        ppu.write_register(PPU_CTRL, 0x04);
        set_ppu_addr(&mut ppu, 0x2000);
        ppu.write_register(PPU_DATA, 0xAA);
        ppu.write_register(PPU_DATA, 0xBB);

        assert_eq!(ppu.ppu_mem_read(0x2000), 0xAA);
        assert_eq!(ppu.ppu_mem_read(0x2020), 0xBB);
        assert_eq!(ppu.ppu_mem_read(0x2001), 0x00);
    }

    #[test]
    fn test_ppustatus_read_clears_vblank_and_latches() {
        let mut ppu = Ppu::new(Mirroring::Horizontal);
        ppu.status = 0x80;
        ppu.write_register(PPU_SCROLL, 0x01);
        ppu.write_register(PPU_ADDR, 0x20);
        assert!(ppu.scroll_latch);
        assert!(ppu.addr_latch);

        let status = ppu.read_register(PPU_STATUS);
        assert_eq!(status & 0x80, 0x80);
        assert_eq!(ppu.status & 0x80, 0x00);
        assert!(!ppu.scroll_latch);
        assert!(!ppu.addr_latch);
    }

    #[test]
    fn test_horizontal_mirroring_maps_2000_and_2400_together() {
        let mut ppu = Ppu::new(Mirroring::Horizontal);

        set_ppu_addr(&mut ppu, 0x2000);
        ppu.write_register(PPU_DATA, 0x11);
        set_ppu_addr(&mut ppu, 0x2400);
        ppu.write_register(PPU_DATA, 0x22);
        set_ppu_addr(&mut ppu, 0x2800);
        ppu.write_register(PPU_DATA, 0x33);

        assert_eq!(ppu.ppu_mem_read(0x2000), 0x22);
        assert_eq!(ppu.ppu_mem_read(0x2400), 0x22);
        assert_eq!(ppu.ppu_mem_read(0x2800), 0x33);
    }

    #[test]
    fn test_vertical_mirroring_maps_2000_and_2800_together() {
        let mut ppu = Ppu::new(Mirroring::Vertical);

        set_ppu_addr(&mut ppu, 0x2000);
        ppu.write_register(PPU_DATA, 0x11);
        set_ppu_addr(&mut ppu, 0x2800);
        ppu.write_register(PPU_DATA, 0x22);
        set_ppu_addr(&mut ppu, 0x2400);
        ppu.write_register(PPU_DATA, 0x33);

        assert_eq!(ppu.ppu_mem_read(0x2000), 0x22);
        assert_eq!(ppu.ppu_mem_read(0x2800), 0x22);
        assert_eq!(ppu.ppu_mem_read(0x2400), 0x33);
    }

    #[test]
    fn test_3000_region_mirrors_2000_region() {
        let mut ppu = Ppu::new(Mirroring::Horizontal);

        set_ppu_addr(&mut ppu, 0x2000);
        ppu.write_register(PPU_DATA, 0x66);

        set_ppu_addr(&mut ppu, 0x3000);
        assert_eq!(ppu.read_register(PPU_DATA), 0x00);
        assert_eq!(ppu.read_register(PPU_DATA), 0x66);
    }

    #[test]
    fn test_palette_special_mirror_3f10_to_3f00() {
        let mut ppu = Ppu::new(Mirroring::Horizontal);

        set_ppu_addr(&mut ppu, 0x3F10);
        ppu.write_register(PPU_DATA, 0x77);

        set_ppu_addr(&mut ppu, 0x3F00);
        assert_eq!(ppu.read_register(PPU_DATA), 0x77);
    }

    #[test]
    fn test_ppudata_palette_read_is_immediate_and_updates_buffer() {
        let mut ppu = Ppu::new(Mirroring::Horizontal);

        set_ppu_addr(&mut ppu, 0x2F00);
        ppu.write_register(PPU_DATA, 0x44);
        set_ppu_addr(&mut ppu, 0x3F00);
        ppu.write_register(PPU_DATA, 0x88);

        set_ppu_addr(&mut ppu, 0x3F00);
        assert_eq!(ppu.read_register(PPU_DATA), 0x88);

        set_ppu_addr(&mut ppu, 0x2000);
        assert_eq!(ppu.read_register(PPU_DATA), 0x44);
    }
}
