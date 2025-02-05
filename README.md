This crate works around the following weakness in standard Rust. It is
generally impossible to write:

```rust
  // Say that you're working with a Weak<RefCell<u32>> for safe, shared
  // mutability.
  let x = Rc::new(RefCell::new(0));
  let x = Rc::downgrade(&x);

  // This doesn't work. The Rc returned by x.upgrade() is dropped at the end
  // of the statement, and therefore ref_inner can't be used.
  let ref_inner = x.upgrade().unwrap().borrow();
  assert_eq!(0, *ref_inner);
```

With this crate, replacing `borrow` with `nest_borrow` Just Works

```rust
  let x = Rc::new(RefCell::new(0));
  let x = Rc::downgrade(&x);

  let ref_inner = x.upgrade().unwrap().nest_borrow();
  assert_eq!(0, *ref_inner);
```

The implementation relies on one `unsafe` function to prevent rustc from
being overly strict.

The implementation guarantees that the "outer temporary" (Rc in the above
example) does in fact outlive the reference to it.

The implementation has no runtime overhead.

Arbitrary nesting depth is supported

```rust
  let x = RefCell::new(RefCell::new(RefCell::new(0)));
  let ref_inner = x.borrow().nest_borrow().nest_borrow();
  assert_eq!(0, *ref_inner);
```

`nest_*` functions also work fine in the initial position of the chain, in
which position they behave like their unprefixed analogues. The following
two lines are identical.

```rust
  let ref_inner = x.     borrow().nest_borrow().nest_borrow();
  let ref_inner = x.nest_borrow().nest_borrow().nest_borrow();
```

If you ever have had to work with the miserable experience of reassigning a
stack of guards, you may recognize that this code doesn't compile

```rust
  let x = RefCell::new(RefCell::new(0));
  let ys: Vec<_> = (1..10).map(|i| RefCell::new(RefCell::new(i))).collect();

  let mut ref1 = x.borrow();
  let mut ref2 = ref1.borrow_mut();

  for y in ys.iter() {
    let mut yref1 = y.borrow();
    let mut yref2 = yref1.borrow_mut();
    mem::swap(ref2.deref_mut(), yref2.deref_mut());
    ref2 = yref2;
    ref1 = yref1;
  }
```

However, the analogue using this crate again Just Works

```rust
  let x = RefCell::new(RefCell::new(0));
  let ys: Vec<_> = (1..10).map(|i| RefCell::new(RefCell::new(i))).collect();

  let mut ref1 = x.borrow().nest_borrow_mut();

  for y in ys.iter() {
    let mut yref = y.borrow().nest_borrow_mut();
    mem::swap(ref1.deref_mut(), yref.deref_mut());
    ref1 = yref;
  }

```

The following methods are provided, via extension traits
```rust
  std::cell::RefCell::nest_borrow
  std::cell::RefCell::nest_borrow_mut
  std::cell::RefCell::try_nest_borrow
  std::cell::RefCell::try_nest_borrow_mut
  std::rc::Weak::nest_upgrade
  std::sync::Weak::nest_upgrade
  std::sync::Mutex::nest_lock
  std::sync::Mutex::nest_try_lock
  std::sync::RwLock::nest_try_read
  std::sync::RwLock::nest_try_write
```
