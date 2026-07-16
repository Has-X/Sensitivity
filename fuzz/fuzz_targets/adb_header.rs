#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() == 24 {
        let mut header = [0_u8; 24];
        header.copy_from_slice(data);
        let _ = sensitivity::adb::decode_header(&header);
    }
});
