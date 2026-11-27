use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct Counting;

thread_local! {
    static CALLS: Cell<Option<usize>> = const { Cell::new(None) };
}

fn count() {
    let _ = CALLS.try_with(|calls| {
        if let Some(so_far) = calls.get() {
            calls.set(Some(so_far + 1));
        }
    });
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        count();
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

pub fn heap_calls(work: impl FnOnce()) -> usize {
    CALLS.with(|calls| calls.set(Some(0)));
    work();
    CALLS.with(|calls| calls.replace(None).unwrap_or(0))
}
