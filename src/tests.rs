// Copyright 2016 taskqueue developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use super::*;

extern crate env_logger;

macro_rules! serial_test {
    ($x:ident, $y:ident, $z:ident, $n:ident => $block:expr) => {
        #[test]
        fn $x()
        {
            let _ = env_logger::init();
            init_main(|queue| $block(queue));
        }

        #[test]
        fn $y()
        {
            let _ = env_logger::init();
            let queue = SerialQueue::new();
            $block(queue);
        }

        #[test]
        fn $z()
        {
            let _ = env_logger::init();
            init_main(|_| {
                let queue = SerialQueue::new();
                $block(queue)
            });
        }

        #[test]
        fn $n()
        {
            let _ = env_logger::init();
            init_main(|_| {
                let queue = SerialQueue::new_native();
                $block(queue)
            });
        }
    }
}

macro_rules! concurrent_test {
    ($c:ident, $d:ident => $block:expr) => {
        #[test]
        fn $c()
        {
            let _ = env_logger::init();
            let queue = ConcurrentQueue::new();
            $block(queue);
        }

        #[test]
        fn $d()
        {
            let _ = env_logger::init();
            init_main(|_| {
                let queue = ConcurrentQueue::new();
                $block(queue)
            });
        }
    }
}

serial_test!(sync_main, sync_nomain, sync_main_new, sync_main_new_native =>
    |queue: SerialQueue|
    {
        let result: u32 = queue.sync(|| 42);
        assert_eq!(result, 42);
    }
);
concurrent_test!(sync_concurrent, sync_concurrent_main =>
    |queue: ConcurrentQueue|
    {
        let result: u32 = queue.sync(|| 42);
        assert_eq!(result, 42);
    }
);

serial_test!(scoped_main, scoped_nomain, scoped_main_new, scoped_main_new_native =>
    |queue: SerialQueue|
    {
        let string = "Test".to_string();
        let future = queue.scoped(|| string.clone());
        assert!(future.get() == string);
    }
);
concurrent_test!(scoped_concurrent, scoped_concurrent_main =>
    |queue: ConcurrentQueue|
    {
        let string = "Test".to_string();
        let future = queue.scoped(|| string.clone());
        assert!(future.get() == string);
    }
);

serial_test!(async_main, async_nomain, async_main_new, async_main_new_native =>
    |queue: SerialQueue|
    {
        let future: Future<u32> = queue.async(|| 42);
        assert!(future.get() == 42);
    }
);
concurrent_test!(async_concurrent, async_concurrent_main =>
    |queue: ConcurrentQueue|
    {
        let future: Future<u32> = queue.async(|| 42);
        assert!(future.get() == 42);
    }
);

serial_test!(async_loop_main, async_loop_nomain, async_loop_main_new, async_loop_main_new_native =>
    |queue: SerialQueue|
    {
        use std::sync::Arc;

        let queue = Arc::new(queue);
        let count = 0;

        fn do_work(queue: Arc<SerialQueue>, count: usize)
        {
            if count < 5000 {
                let clone = queue.clone();
                queue.async(move || do_work(clone, count + 1));
            }
        }

        let clone = queue.clone();
        queue.async(move || do_work(clone, count));
    }
);

concurrent_test!(async_loop_concurrent, async_loop_concurrent_main =>
    |queue: ConcurrentQueue|
    {
        use std::sync::Arc;

        let queue = Arc::new(queue);
        let count = 0;

        fn do_work(queue: Arc<ConcurrentQueue>, count: usize)
        {
            if count < 5000 {
                let clone = queue.clone();
                queue.async(move || do_work(clone, count + 1));
            }
        }

        let clone = queue.clone();
        queue.async(move || do_work(clone, count));
    }
);

serial_test!(deadlock_main, deadlock_nomain, deadlock_main_new, deadlock_main_new_native =>
    |queue: SerialQueue|
    {
        let first_queue = queue;
        let second_queue = SerialQueue::new();
        let result = first_queue.sync(|| second_queue.sync(|| first_queue.sync(|| 42)));
        assert_eq!(result, 42);
    }
);

concurrent_test!(deadlock_concurrent, deadlock_concurrent_main =>
    |queue: ConcurrentQueue|
    {
        let first_queue = queue;
        let second_queue = SerialQueue::new();
        let result = first_queue.sync(|| second_queue.sync(|| first_queue.sync(|| 42)));
        assert_eq!(result, 42);
    }
);
concurrent_test!(deadlock_concurrent_new, deadlock_concurrent_main_new =>
    |queue: ConcurrentQueue|
    {
        let first_queue = queue;
        let second_queue = ConcurrentQueue::new();
        let result = first_queue.sync(|| second_queue.sync(|| first_queue.sync(|| 42)));
        assert_eq!(result, 42);
    }
);

serial_test!(hidden_deadlock_main, hidden_deadlock_nomain, hidden_deadlock_main_new, hidden_deadlock_main_new_native =>
    |main: SerialQueue|
    {
        let queue = SerialQueue::new();
        let result = queue.sync(|| {
            let future = queue.async(|| 42);
            main.async(move || {
                future.get()
            }).get()
        });
        assert_eq!(result, 42);
    }
);
concurrent_test!(hidden_deadlock_concurrent, hidden_deadlock_concurrent_main =>
    |main: ConcurrentQueue|
    {
        let queue = SerialQueue::new();
        let result = queue.sync(|| {
            let future = queue.async(|| 42);
            main.async(move || {
                future.get()
            }).get()
        });
        assert_eq!(result, 42);
    }
);
concurrent_test!(hidden_deadlock_concurrent_two, hidden_deadlock_concurrent_main_two =>
    |main: ConcurrentQueue|
    {
        let queue = ConcurrentQueue::new();
        let result = queue.sync(|| {
            let future = queue.async(|| 42);
            main.async(move || {
                future.get()
            }).get()
        });
        assert_eq!(result, 42);
    }
);

serial_test!(triple_deadlock_main, triple_deadlock_nomain, triple_deadlock_main_new, triple_deadlock_main_new_native =>
    |queue: SerialQueue|
    {
        let result = queue.sync(|| queue.sync(|| queue.sync(|| 42)));
        assert_eq!(result, 42);
    }
);
concurrent_test!(triple_deadlock_concurrent, triple_deadlock_concurrent_main =>
    |queue: ConcurrentQueue|
    {
        let result = queue.sync(|| queue.sync(|| queue.sync(|| 42)));
        assert_eq!(result, 42);
    }
);

serial_test!(simple_group_main, simple_group_nomain, simple_group_main_new, simple_group_main_new_native =>
    |queue: SerialQueue|
    {
        let queue2 = SerialQueue::new();
        let mut group = Group::new();
        group.async(&queue, || { 21 });
        group.async(&queue2, || { 21 });
        let result = group.wait().fold(0, |x, y| {
            x+y
        });
        assert_eq!(result, 42);
    }
);
concurrent_test!(simple_group_concurrent, simple_group_concurrent_main =>
    |queue: ConcurrentQueue|
    {
        let queue2 = SerialQueue::new();
        let mut group = Group::new();
        group.async(&queue, || { 21 });
        group.async(&queue2, || { 21 });
        let result = group.wait().fold(0, |x, y| {
            x+y
        });
        assert_eq!(result, 42);
    }
);
concurrent_test!(simple_group_concurrent_two, simple_group_concurrent_main_two =>
    |queue: ConcurrentQueue|
    {
        let queue2 = ConcurrentQueue::new();
        let mut group = Group::new();
        group.async(&queue, || { 21 });
        group.async(&queue2, || { 21 });
        let result = group.wait().fold(0, |x, y| {
            x+y
        });
        assert_eq!(result, 42);
    }
);

serial_test!(simple_bound_serial_main, simple_bound_serial_nomain, simple_bound_serial_main_new, simple_bound_serial_main_new_native =>
    |queue: SerialQueue|
    {
        let bound = queue.with(|| { 42 });
        bound.scoped_with(|x| *x+=1);
        bound.scoped_with(|x| *x+=1);
        bound.sync_with(|x| { assert_eq!(*x, 44) });
    }
);

serial_test!(foreach_bound_serial_main, foreach_bound_serial_nomain, foreach_bound_serial_main_new, foreach_bound_serial_main_new_native =>
    |queue: SerialQueue|
    {
        let mut i = 1;
        let bound = queue.with(|| 1);
        for x in bound.foreach_with(0..42, |i, x| x + i.clone()) {
            assert_eq!(x, i);
            i += 1;
        }
        assert_eq!(43, i);
    }
);

serial_test!(foreach_serial_main, foreach_serial_nomain, foreach_serial_main_new, foreach_serial_main_new_native =>
    |queue: SerialQueue|
    {
        let mut i = 1;
        for x in queue.foreach(0..42, |x| x+1) {
            assert_eq!(x, i);
            i += 1;
        }
        assert_eq!(43, i);
    }
);
concurrent_test!(foreach_serial_concurrent, foreach_serial_concurrent_main =>
    |queue: ConcurrentQueue|
    {
        let mut i = 1;
        for x in queue.foreach(0..42, |x| x+1) {
            assert_eq!(x, i);
            i += 1;
        }
        assert_eq!(43, i);
    }
);

serial_test!(loop_while_bound_serial_main, loop_while_bound_serial_nomain, loop_while_bound_serial_main_new, loop_while_bound_serial_main_new_native =>
    |queue: SerialQueue|
    {
        let result = queue.with(|| 0).loop_while_with(move |i| {
            *i += 1;
            if *i == 42 {
                LoopResult::Done(*i)
            } else {
                LoopResult::Continue
            }
        }).get();
        assert_eq!(result, 42);
    }
);

serial_test!(loop_while_serial_main, loop_while_serial_nomain, loop_while_serial_main_new, loop_while_serial_main_new_native =>
    |queue: SerialQueue|
    {
        use std::sync::Arc;
        use io::sync::Mutex;

        let i = Arc::new(Mutex::new(0));
        let result = queue.loop_while(move || {
            let mut count = i.lock().unwrap();
            *count += 1;
            if *count == 42 {
                LoopResult::Done(*count)
            } else {
                LoopResult::Continue
            }
        }).get();
        assert_eq!(result, 42);
    }
);
concurrent_test!(loop_while_concurrent, loop_while_concurrent_main =>
    |queue: ConcurrentQueue|
    {
        use std::sync::Arc;
        use io::sync::Mutex;

        let i = Arc::new(Mutex::new(0));
        let result = queue.loop_while(move || {
            let mut count = i.lock().unwrap();
            *count += 1;
            if *count == 42 {
                LoopResult::Done(*count)
            } else {
                LoopResult::Continue
            }
        }).get();
        assert_eq!(result, 42);
    }
);

//also serves as foreach_concurrent_main
serial_test!(iterator_main, iterator_nomain, iterator_main_new, iterator_main_new_native =>
    |_|
    {
        let mut i = 1;
        for x in (0..42).concurrent_map(|x| { x+1 }) {
            assert_eq!(x, i);
            i += 1;
        }
        assert_eq!(43, i);
    }
);



#[test]
#[should_panic]
fn fail_on_inner_panic() {
    let _ = env_logger::init();
    init_main(|queue| {
        queue.sync(|| { panic!() });
    });
}
