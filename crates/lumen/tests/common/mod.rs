#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};

pub fn swf_files() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/libsrc/ffdec_lib/testdata")];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("swf")) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn threads() -> usize {
    std::env::var("LUMEN_TEST_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get()))
}

/// Work permits shared by every test in the binary, so parallel tests don't multiply the thread count.
fn permits() -> &'static (Mutex<usize>, Condvar) {
    static P: OnceLock<(Mutex<usize>, Condvar)> = OnceLock::new();
    P.get_or_init(|| (Mutex::new(threads()), Condvar::new()))
}

/// Maps `f` over `items` in parallel and returns the results in item order.
pub fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|s| {
        for _ in 0..threads().min(items.len()) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= items.len() {
                    break;
                }
                let (lock, cv) = permits();
                let mut n = cv.wait_while(lock.lock().unwrap(), |n| *n == 0).unwrap();
                *n -= 1;
                drop(n);
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&items[i])));
                *lock.lock().unwrap() += 1;
                cv.notify_one();
                match r {
                    Ok(r) => out.lock().unwrap()[i] = Some(r),
                    Err(e) => std::panic::resume_unwind(e),
                }
            });
        }
    });
    out.into_inner().unwrap().into_iter().map(Option::unwrap).collect()
}

/// `par_map` over the parsed corpus SWFs, skipping files that don't parse.
pub fn par_swfs<R: Send>(f: impl Fn(&Path, lumen::Swf) -> R + Sync) -> Vec<R> {
    par_map(&swf_files(), |p| std::fs::read(p).ok().and_then(|d| lumen::Swf::parse(&d).ok()).map(|s| f(p, s))).into_iter().flatten().collect()
}
