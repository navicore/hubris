// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! HIF interpreter
//!
//! HIF is the Hubris/Humility Interchange Format, a simple stack-based
//! machine that allows for some dynamic programmability of Hubris.  In
//! particular, this task provides a HIF interpreter to allow for Humility
//! commands like `humility i2c`, `humility pmbus` and `humility jefe`.  The
//! debugger places HIF in [`HIFFY_TEXT`], and then indicates that text is
//! present by incrementing [`HIFFY_KICK`].  This task executes the specified
//! HIF, with the return stack located in [`HIFFY_RSTACK`].

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering};
use hif::*;
use userlib::*;

mod common;

cfg_if::cfg_if! {
    if #[cfg(feature = "stm32h7")] {
        pub mod stm32h7;
        use crate::stm32h7::*;
    } else if #[cfg(feature = "lpc55")] {
        pub mod lpc55;
        use crate::lpc55::*;
    } else {
        pub mod generic;
        use crate::generic::*;
    }
}

cfg_if::cfg_if! {
    if #[cfg(any(
        target_board = "gimlet-1",
        target_board = "gimletlet-2",
        target_board = "nucleo-h743zi2",
        target_board = "nucleo-h753zi"
    ))] {
        const HIFFY_DATA_SIZE: usize = 20_480;
    } else {
        const HIFFY_DATA_SIZE: usize = 2_048;
    }
}

///
/// These HIFFY_* global variables constitute the interface with Humility;
/// they should not be altered without modifying Humility as well.
///
/// - [`HIFFY_TEXT`]       => Program text for HIF operations
/// - [`HIFFY_RSTACK`]     => HIF return stack
/// - [`HIFFY_REQUESTS`]   => Count of succesful requests
/// - [`HIFFY_ERRORS`]     => Count of HIF execution failures
/// - [`HIFFY_FAILURE`]    => Most recent HIF failure, if any
/// - [`HIFFY_KICK`]       => Variable that will be written to to indicate that
///                           [`HIFFY_TEXT`] contains valid program text
/// - [`HIFFY_READY`]      => Variable that will be non-zero iff the HIF
///                           execution engine is waiting to be kicked
///
static mut HIFFY_TEXT: [u8; 2048] = [0; 2048];
static mut HIFFY_DATA: [u8; HIFFY_DATA_SIZE] = [0; HIFFY_DATA_SIZE];
static mut HIFFY_RSTACK: [u8; 2048] = [0; 2048];
static HIFFY_REQUESTS: AtomicU32 = AtomicU32::new(0);
static HIFFY_ERRORS: AtomicU32 = AtomicU32::new(0);
static HIFFY_KICK: AtomicU32 = AtomicU32::new(0);
static HIFFY_READY: AtomicU32 = AtomicU32::new(0);

#[used]
static mut HIFFY_FAILURE: Option<Failure> = None;

///
/// We deliberately export the HIF version numbers to allow Humility to
/// fail cleanly if its HIF version does not match our own.
///
static HIFFY_VERSION_MAJOR: AtomicU32 = AtomicU32::new(HIF_VERSION_MAJOR);
static HIFFY_VERSION_MINOR: AtomicU32 = AtomicU32::new(HIF_VERSION_MINOR);
static HIFFY_VERSION_PATCH: AtomicU32 = AtomicU32::new(HIF_VERSION_PATCH);

#[export_name = "main"]
fn main() -> ! {
    let mut sleep_ms = 250;
    let mut sleeps = 0;
    let mut stack = [None; 32];
    let mut scratch = [0u8; 256];
    const NLABELS: usize = 4;

    //
    // Sadly, there seems to be no other way to force these variables to
    // not be eliminated...
    //
    HIFFY_VERSION_MAJOR.fetch_add(0, Ordering::SeqCst);
    HIFFY_VERSION_MINOR.fetch_add(0, Ordering::SeqCst);
    HIFFY_VERSION_PATCH.fetch_add(0, Ordering::SeqCst);

    loop {
        HIFFY_READY.fetch_add(1, Ordering::SeqCst);
        hl::sleep_for(sleep_ms);
        HIFFY_READY.fetch_sub(1, Ordering::SeqCst);

        if HIFFY_KICK.load(Ordering::SeqCst) == 0 {
            sleeps += 1;

            // Exponentially backoff our sleep value, but no more than 250ms
            if sleeps == 10 {
                sleep_ms = core::cmp::min(sleep_ms * 10, 250);
                sleeps = 0;
            }

            continue;
        }

        //
        // Whenever we have been kicked, we adjust our timeout down to 1ms,
        // from which we will exponentially backoff
        //
        HIFFY_KICK.fetch_sub(1, Ordering::SeqCst);
        sleep_ms = 1;
        sleeps = 0;

        let text = unsafe { &HIFFY_TEXT };
        let data = unsafe { &HIFFY_DATA };
        let mut rstack = unsafe { &mut HIFFY_RSTACK[0..] };

        let check = |offset: usize, op: &Op| -> Result<(), Failure> {
            trace_execute(offset, *op);
            Ok(())
        };

        let rv = execute::<_, NLABELS>(
            text,
            HIFFY_FUNCS,
            data,
            &mut stack,
            &mut rstack,
            &mut scratch,
            check,
        );

        match rv {
            Ok(_) => {
                HIFFY_REQUESTS.fetch_add(1, Ordering::SeqCst);
                trace_success();
            }
            Err(failure) => {
                HIFFY_ERRORS.fetch_add(1, Ordering::SeqCst);
                unsafe {
                    HIFFY_FAILURE = Some(failure);
                }

                trace_failure(failure);
            }
        }
    }
}