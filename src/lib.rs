// Copyright 2016 taskqueue developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//!
//! # TaskQueue
//!
//! Multi-threading for Rust made easy based on [*mioco*](https://github.com/dpc/mioco)
//! ===================================================================================
//!
//! TaskQueue is a libdispatch inspired threading library.
//!
//! Instead of manually managing threads, your create Queues with handle a set of tasks
//! each with their own invariants and guarantees regarding those tasks.
//! The Queues themselves are very lightweight and run on a shared global thread pool,
//! that is completely managed in the background by the libary, allowing you to abstract easily
//! and focus on your particular problem.
//!
//! TaskQueue provides you with two types of queues:
//!
//! 1. [*SerialQueues*](./struct.SerialQueue.html) - executing tasks serialily non-overlapping in order of queuing
//! 2. [*ConcurrentQueues*](./struct.ConcurrentQueue.html) - executing tasks parallely on any thread as soon as possible
//!
//! and an IO module derived from [*mioco*](https://github.com/dpc/mioco) that lets you use
//! non-blocking IO (which is required in queues) in a blocking fashion.
//!

#![feature(scoped_tls, fnbox)]

//#![recursion_limit="128"]
#![deny(missing_debug_implementations, missing_copy_implementations,
        trivial_casts, trivial_numeric_casts,
        unused_import_braces, unused_qualifications,
        missing_docs)]

#[macro_use]
extern crate log;

#[macro_use]
extern crate mioco;

#[macro_use]
extern crate lazy_static;

/// IO module providing a queue-compatible mostly std-like replacement for native blocking IO.
/// Provided through mioco and mio. Other types implementing mio's Evented Trait might be used
/// aswell through io::mio::MioAdapter.
///
/// With queues executing on a thread pool, normal blocking IO is not working anymore.
/// Anything blocking may block multiple queues at once causing a deadlock or starvation.
///
/// In cases where blocking IO cannot be avoided, a solution is provided through `SerialQueue::new_native()`.
pub mod io
{
    pub use mioco::MioAdapter;
    pub use mioco::sync;
    pub use mioco::mail;
    pub use mioco::timer;
    pub use mioco::udp;
    pub use mioco::tcp;
    pub use mioco::unix;
}

mod future;
mod queue;
mod group;
mod util;
pub mod extensions;

pub use self::queue::{ConcurrentQueue, SerialQueue, BoundSerialQueue, Queue, LoopResult, init_main};
pub use self::future::{Future, FutureGuard};
pub use self::group::Group;
pub use self::extensions::iterator::*;

#[cfg(test)]
mod tests;
