// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

#![no_std]
#![no_main]

use drv_sp_ctrl_api::*;
use ringbuf::*;
use sha2::{Digest, Sha256};
use userlib::*;

const READ_SIZE: usize = 256;

const TRANSACTION_SIZE: u32 = 1024;

task_slot!(SP_CTRL, swd);

#[derive(Copy, Clone, PartialEq)]
struct ShaOut {
    out: [u8; 32],
}

#[derive(Copy, Clone, PartialEq)]
enum Trace {
    HashOut(ShaOut),
    ErrCnt(usize),
    Addr(u32),
    Start(u64),
    End(u64),
    Data([u8; READ_SIZE]),
    // addr, offset, got, expected
    Badness10000(u32, usize, u8, u8),
    None,
}

ringbuf!(Trace, 16, Trace::None);

fn cmp(a: &[u8], b: &[u8]) -> Option<(usize, u8, u8)> {
    if a.len() != b.len() {
        loop {}
    }

    for i in 0..a.len() {
        if a[i] != b[i] {
            return Some((i, a[i], b[i]));
        }
    }

    None
}

#[export_name = "main"]
fn main() -> ! {
    let mut err_cnt = 0;
    loop {
        let mut sha = Sha256::new();
        let sp_ctrl = SpCtrl::from(SP_CTRL.get_task_id());

        match sp_ctrl.setup() {
            Err(_) => loop {},
            _ => (),
        }

        let mut data: [u8; READ_SIZE] = [0; READ_SIZE];

        let start = sys_get_timer().now;
        ringbuf_entry!(Trace::Start(start));
        for (i, addr) in (FLASH_START..FLASH_END).step_by(READ_SIZE).enumerate()
        {
            if addr % TRANSACTION_SIZE == 0 {
                loop {
                    match sp_ctrl.read_transaction_start(addr, addr + TRANSACTION_SIZE) {
                        Err(_) => {
                            err_cnt += 1;
                            let _ = sp_ctrl.setup();
                            continue;
                        }
                        _ => break,
                    }
                }
            }

            data.fill(0);
            loop {
                match sp_ctrl.read_transaction(&mut data) {
                    Err(_) => {
                        ringbuf_entry!(Trace::Addr(addr));
                        ringbuf_entry!(Trace::Data(data));
                        loop {
                            match sp_ctrl.setup() {
                                Err(_) => continue,
                                Ok(_) => {
                                    err_cnt += 1;
                                    match sp_ctrl
                                        .read_transaction_start(addr, FLASH_END)
                                    {
                                        Err(_) => continue,
                                        Ok(_) => break,
                                    }
                                }
                            }
                        }
                    }
                    Ok(_) => break,
                }
            }

            let bit: usize = i * READ_SIZE;

            if let Some((i, a, b)) =
                cmp(&data, &EXPECTED_BYTES[bit..(bit + READ_SIZE)])
            {
                ringbuf_entry!(Trace::Data(data));
                ringbuf_entry!(Trace::ErrCnt(err_cnt));
                ringbuf_entry!(Trace::Badness10000(addr, i, a, b));
                loop {}
            }
            sha.update(&data);
        }

        let sha_out = sha.finalize();

        let mut log = ShaOut { out: [0; 32] };

        let end = sys_get_timer().now;
        ringbuf_entry!(Trace::End(end));
        log.out.copy_from_slice(&sha_out);

        ringbuf_entry!(Trace::ErrCnt(err_cnt));
        ringbuf_entry!(Trace::HashOut(log));
    }
}

include!(concat!(env!("OUT_DIR"), "/expected.rs"));
