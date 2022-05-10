// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::io::Write;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct TaskConfig {
    binary_path : PathBuf,
}

const TEST_SIZE: usize = 0x1_0000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::env::var("OUT_DIR")?;
    let dest_path = std::path::Path::new(&out_dir).join("expected.rs");
    let mut file = std::fs::File::create(&dest_path)?;

    let task_config = build_util::task_config::<TaskConfig>()?;

    let bin = std::fs::read(
        &task_config.binary_path
    )
    .unwrap();

    writeln!(&mut file, "const FLASH_START: u32 = 0x0800_0000;").unwrap();
    writeln!(&mut file, "const TEST_SIZE: u32 = {};", TEST_SIZE).unwrap();
    writeln!(&mut file, "const FLASH_END: u32 = FLASH_START + TEST_SIZE;")
        .unwrap();

    writeln!(&mut file, "static EXPECTED_BYTES: [u8; {}] = [", TEST_SIZE)
        .unwrap();
    for b in &bin[..TEST_SIZE] {
        writeln!(&mut file, "0x{:x},", b).unwrap();
    }

    writeln!(&mut file, "];").unwrap();
    Ok(())
}
