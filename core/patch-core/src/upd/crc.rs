//! CRC-16/XMODEM (poly 0x1021, init 0, no reflection, no final xor) used as the
//! per-document trailer in `.UPD` containers.

const POLY: u16 = 0x1021;
const TABLE: [u16; 256] = build_table();

const fn build_table() -> [u16; 256] {
    let mut table = [0u16; 256];
    let mut index = 0;
    while index < 256 {
        let mut crc = (index as u16) << 8;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ POLY
            } else {
                crc << 1
            };
            bit += 1;
        }
        table[index] = crc;
        index += 1;
    }
    table
}

/// CRC-16/XMODEM of `data`. Check value: `crc16_xmodem(b"123456789") == 0x31C3`.
pub fn crc16_xmodem(data: &[u8]) -> u16 {
    data.iter().fold(0u16, |crc, &byte| {
        let index = usize::from(((crc >> 8) as u8) ^ byte);
        (crc << 8) ^ TABLE[index]
    })
}
