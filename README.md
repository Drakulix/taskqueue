# TaskQueue [![Build Status](https://travis-ci.org/Drakulix/taskqueue.svg?branch=master)](https://travis-ci.org/Drakulix/taskqueue) [![Build status](https://ci.appveyor.com/api/projects/status/b77qdi7rbhraxdvm?svg=true)](https://ci.appveyor.com/project/Drakulix/taskqueue) [![Coverage Status](https://coveralls.io/repos/github/drakulix/taskqueue/badge.svg?branch=master)](https://coveralls.io/github/drakulix/taskqueue?branch=master) [![Crates.io](https://img.shields.io/crates/v/taskqueue.svg)](https://crates.io/crates/taskqueue) [![Crates.io](https://img.shields.io/crates/l/taskqueue.svg)]()



Multi-threading for Rust made easy based on [*mioco*](https://github.com/dpc/mioco)
===================================================================================

TaskQueue is a libdispatch inspired threading library.

Instead of manually managing threads, your create Queues with handle a set of tasks each with their own invariants and guarantees regarding those tasks.
The Queues themselves are very lightweight and run on a shared global thread pool, that is completely managed in the background by the library, allowing you to abstract easily and focus on your particular problem.

## Motivation

Multi-threading is difficult, but it does not need to be.

What do you want from Thread?
You want them to
    -  utilize more cores (at best automatically)
    -  execute certain operations.

What you usually don't want to do, is
    - managing their run loop
    - worry about deadlocks
    - worry about overlapping operations

So what your thread-like object really should be is a queue
for tasks with the following guarantees:
    - only one task executes at a time
    - all execute in queuing order
    - a queue is bond to a specific underlying thread for safe interactions with not thread-safe types.

Sounds good to you?

## Example

```rust
extern crate taskqueue;

use taskqueue::*;

fn main() {
    init_main(|main| {
        let thread_one = SerialQueue::new();
        let thread_two = SerialQueue::new();

        let future_one = thread_one.async(|| {
            42
        });
        let future_two = thread_two.async(|| {
            96
        });

        println!("Although this is happening in main,");
        main.async(|| {
            println!("this task is running before...");
        });
        main.sync(|| {
            println!("...this task and...");
            assert_eq!(future_one.get() + future_two.get(), 138);
        });
        println!("...this is running last");
    });
}
```

Also there is another Queue `ConcurrentQueue` which equally splits all tasks among
the thread pool, that is handled by the library. The allows for
quick parallelization without creating thousands of queues.

## Getting Started

Just add
```
[dependencies]
taskqueue = "^1.0.0"
```
to your `Cargo.toml`

## [Documentation](https://drakulix.github.io/taskqueue/taskqueue/index.html)

## Contributing

Contributions are highly welcome, but I suggest you contact me about larger changes you
may want to implement at first so you don't waste your time. To do that you may either:
    - fill an issue about the missing feature or bug you want to solve.
    - contact me via Gitter

Then just:
1. Fork it!
2. Create your feature branch: git checkout -b my-new-feature
3. Commit your changes: git commit -am 'Add some feature'
4. Push to the branch: git push origin my-new-feature
5. Submit a pull request :D

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
