// Copyright 2016 taskqueue developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! Extension of the Iterator Trait

use {ConcurrentQueue, Queue};

use std::vec::IntoIter;

/// The ConcurrentMap Trait adds a `map` like function to
/// iterators that utilizes TaskQueue to execute it in parallel.
pub trait ConcurrentMap: Iterator + Send + Sized
 {
    /// `map` like function utilizing a ConcurrentQueue internally
    /// to execute it parallely returning a Group which also implements
    /// the Iterator Trait and holds the results in order.
    /// 
    /// # Example
    ///
    /// ```
    /// # use taskqueue::*;
    /// let squared: Vec<i32> = (0..20).concurrent_map(|x| x*x).collect();
    /// # assert_eq!((0..20).map(|x| x*x).collect::<Vec<i32>>(), squared);
    /// ```
    fn concurrent_map<B, F>(self, f: F) -> IntoIter<B>
        where F: Fn(Self::Item) -> B + Send + Sync,
              B: Send + 'static;
}

impl<R: Send, I: Iterator<Item = R> + Send + Sized + 'static> ConcurrentMap for I
{
    fn concurrent_map<B, F>(self, f: F) -> IntoIter<B>
        where F: Fn(Self::Item) -> B + Send + Sync,
              B: Send + 'static
    {
        let queue = ConcurrentQueue::new();
        queue.foreach(self, f).wait()
    }
}
