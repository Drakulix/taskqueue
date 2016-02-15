// Copyright 2016 taskqueue developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

pub struct NotThreadSafe<T: Sized + 'static>(*mut T);

unsafe impl<T: Sized + 'static> Send for NotThreadSafe<T>
{}

unsafe impl<T: Sized + 'static> Sync for NotThreadSafe<T>
{}

impl<T: Sized + 'static> Clone for NotThreadSafe<T>
{
    fn clone(&self) -> Self
    {
        NotThreadSafe(self.0)
    }
}

impl<T: Sized + 'static> NotThreadSafe<T>
{
    pub fn new(val: T) -> NotThreadSafe<T>
    {
        NotThreadSafe(Box::into_raw(Box::new(val)))
    }
    pub unsafe fn get_mut(&self) -> &mut T
    {
        &mut *self.0
    }
    pub unsafe fn drop(self) -> T
    {
        *Box::from_raw(self.0)
    }
}
