//! This crate works around the following weakness in standard Rust. It is
//! generally impossible to write:
//!
//!   // Say that you're working with a Weak<RefCell<u32>> for safe, shared
//!   // mutability.
//!   let x = Rc::new(RefCell::new(0));
//!   let x = Rc::downgrade(&x);
//!
//!   // This doesn't work. The Rc returned by x.upgrade() is dropped at the end
//!   // of the statement, and therefore ref_inner can't be used.
//!   let ref_inner = x.upgrade().unwrap().borrow();
//!   assert_eq!(0, *ref_inner);
//!
//! With this crate, replacing `borrow` with `nest_borrow` Just Works
//!
//!   let x = Rc::new(RefCell::new(0));
//!   let x = Rc::downgrade(&x);
//!
//!   let ref_inner = x.upgrade().unwrap().nest_borrow();
//!   assert_eq!(0, *ref_inner);
//!
//! The implementation relies on one `unsafe` function to prevent rustc from
//! being overly strict.
//!
//! The implementation guarantees that the "outer temporary" (Rc in the above
//! example) does in fact outlive the reference to it.
//!
//! The implementation has no runtime overhead.
//!
//! Arbitrary nesting depth is supported
//!
//!   let x = RefCell::new(RefCell::new(RefCell::new(0)));
//!   let ref_inner = x.borrow().nest_borrow().nest_borrow();
//!   assert_eq!(0, *ref_inner);
//!
//! `nest_*` functions also work fine in the initial position of the chain, in
//! which position they behave like their unprefixed analogues. The following
//! two lines are identical.
//!
//!   let ref_inner = x.     borrow().nest_borrow().nest_borrow();
//!   let ref_inner = x.nest_borrow().nest_borrow().nest_borrow();
//!
//! If you ever have had to work with the miserable experience of reassigning a
//! stack of guards, you may recognize that this code doesn't compile
//!
//!   let x = RefCell::new(RefCell::new(0));
//!   let ys: Vec<_> = (1..10).map(|i| RefCell::new(RefCell::new(i))).collect();
//!
//!   let mut ref1 = x.borrow();
//!   let mut ref2 = ref1.borrow_mut();
//!
//!   for y in ys.iter() {
//!     let mut yref1 = y.borrow();
//!     let mut yref2 = yref1.borrow_mut();
//!     mem::swap(ref2.deref_mut(), yref2.deref_mut());
//!     ref2 = yref2;
//!     ref1 = yref1;
//!   }
//!
//! However, the analogue using this crate again Just Works
//!
//!   let x = RefCell::new(RefCell::new(0));
//!   let ys: Vec<_> = (1..10).map(|i| RefCell::new(RefCell::new(i))).collect();
//!
//!   let mut ref1 = x.borrow().nest_borrow_mut();
//!
//!   for y in ys.iter() {
//!     let mut yref = y.borrow().nest_borrow_mut();
//!     mem::swap(ref1.deref_mut(), yref.deref_mut());
//!     ref1 = yref;
//!   }
//!
//! The following methods are provided, via extension traits
//!   std::cell::RefCell::nest_borrow
//!   std::cell::RefCell::nest_borrow_mut
//!   std::cell::RefCell::try_nest_borrow
//!   std::cell::RefCell::try_nest_borrow_mut
//!   std::rc::Weak::nest_upgrade
//!   std::sync::Weak::nest_upgrade
//!   std::sync::Mutex::nest_lock
//!   std::sync::Mutex::nest_try_lock
//!   std::sync::RwLock::nest_try_read
//!   std::sync::RwLock::nest_try_write

use std::ops::{Deref, DerefMut};

pub use self::cell::*;
pub use self::rc::*;
pub use self::sync::*;

/// NOTE that drop order is guaranteed first-to-last, so the inner ref is
/// dropped before the outer ref, which we rely on in our unsafe transmutes.
pub struct Nested<T, Inner: Deref<Target = T>, Outer> {
    inner: Inner,
    #[allow(dead_code)]
    outer: Outer,
}
impl<T, Inner: Deref<Target = T>, Outer> Deref for Nested<T, Inner, Outer> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.inner.deref()
    }
}
impl<T, Inner: DerefMut<Target = T>, Outer> DerefMut for Nested<T, Inner, Outer> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.deref_mut()
    }
}

/// # Safety
///   At each usage site in this file, the justification is the same. We work
///   around rustc's inability to understand self-referential structs by casting
///   away the lifetime, and manually ensure that the referenced object outlives
///   the reference by bundling them both into the `Nested` struct.
unsafe fn remove_lifetime<'b, T>(x: &T) -> &'b T {
    unsafe { &*(x as *const T) }
}

mod cell {
    use std::cell::*;

    use super::*;

    pub trait NestedRefCell<'a, T>: Deref<Target = RefCell<T>> + Sized + 'a {
        fn nest_borrow(self) -> Nested<T, Ref<'a, T>, Self> {
            let me = unsafe { remove_lifetime(&self) };
            let inner = RefCell::borrow(me);
            Nested { inner, outer: self }
        }
        fn nest_try_borrow(self) -> Result<Nested<T, Ref<'a, T>, Self>, BorrowError> {
            let me = unsafe { remove_lifetime(&self) };
            let inner = RefCell::try_borrow(me)?;
            Ok(Nested { inner, outer: self })
        }
        fn nest_borrow_mut(self) -> Nested<T, RefMut<'a, T>, Self> {
            let me = unsafe { remove_lifetime(&self) };
            let inner = RefCell::borrow_mut(me);
            Nested { inner, outer: self }
        }
        fn nest_try_borrow_mut(self) -> Result<Nested<T, RefMut<'a, T>, Self>, BorrowMutError> {
            let me = unsafe { remove_lifetime(&self) };
            let inner = RefCell::try_borrow_mut(me)?;
            Ok(Nested { inner, outer: self })
        }
    }
    impl<'a, T, Outer: Deref<Target = RefCell<T>> + Sized + 'a> NestedRefCell<'a, T> for Outer {}

    #[cfg(test)]
    mod test {
        use super::*;

        #[test]
        fn test_many_refcell() {
            let x = RefCell::new(RefCell::new(RefCell::new(0)));
            {
                let z = x.borrow().nest_borrow().nest_borrow();
                assert_eq!(0, *z);
            }
            {
                let z = x
                    .try_borrow()
                    .unwrap()
                    .nest_try_borrow()
                    .unwrap()
                    .nest_try_borrow()
                    .unwrap();
                assert_eq!(0, *z);
            }
            {
                let mut z = x.borrow().nest_borrow().nest_borrow_mut();
                *z = 1;
                assert_eq!(1, *z);
            }
            {
                {
                    let mut y = x.borrow().nest_borrow_mut();
                    *y = RefCell::new(2);
                }
                let z = x.borrow().nest_borrow().nest_borrow();
                assert_eq!(2, *z);
            }
        }
    }
}
mod rc {
    use std::rc::*;

    use super::*;

    pub trait NestedRcWeak<'a, T>: Deref<Target = Weak<T>> + Sized + 'a {
        fn nest_upgrade(self) -> Option<Nested<T, Rc<T>, Self>> {
            let inner = Weak::upgrade(&self)?;
            Some(Nested { inner, outer: self })
        }
    }
    impl<'a, T, Outer: Deref<Target = Weak<T>> + Sized + 'a> NestedRcWeak<'a, T> for Outer {}
    #[cfg(test)]
    mod test {
        use super::*;

        #[test]
        fn test_many_rc() {
            let x1: Rc<i32> = Rc::new(0);
            let y1: Weak<i32> = Rc::downgrade(&x1);

            let x2: Rc<Weak<i32>> = Rc::new(y1);
            let y2: Weak<Weak<i32>> = Rc::downgrade(&x2);

            let x3: Rc<Weak<Weak<i32>>> = Rc::new(y2);
            let y3: Weak<Weak<Weak<i32>>> = Rc::downgrade(&x3);

            let z = y3
                .upgrade()
                .unwrap()
                .nest_upgrade()
                .unwrap()
                .nest_upgrade()
                .unwrap();
            assert_eq!(0, *z);
        }
    }
}

mod sync {
    use std::sync::*;

    use super::*;

    pub trait NestedArcWeak<'a, T>: Deref<Target = Weak<T>> + Sized + 'a {
        fn nest_upgrade(self) -> Option<Nested<T, Arc<T>, Self>> {
            let inner = Weak::upgrade(&self)?;
            Some(Nested { inner, outer: self })
        }
    }
    impl<'a, T, Outer: Deref<Target = Weak<T>> + Sized + 'a> NestedArcWeak<'a, T> for Outer {}

    pub trait NestedMutex<'a, T>: Deref<Target = Mutex<T>> + Sized + 'a {
        fn nest_lock(self) -> LockResult<Nested<T, MutexGuard<'a, T>, Self>> {
            let me = unsafe { remove_lifetime(&self) };
            match me.lock() {
                Ok(inner) => Ok(Nested { inner, outer: self }),
                Err(err) => {
                    let inner = err.into_inner();
                    Err(PoisonError::new(Nested { inner, outer: self }))
                }
            }
        }
        fn nest_try_lock(self) -> TryLockResult<Nested<T, MutexGuard<'a, T>, Self>> {
            let me = unsafe { remove_lifetime(&self) };
            match me.try_lock() {
                Ok(inner) => Ok(Nested { inner, outer: self }),
                Err(err) => match err {
                    TryLockError::Poisoned(err) => {
                        let inner = err.into_inner();
                        Err(TryLockError::Poisoned(PoisonError::new(Nested {
                            inner,
                            outer: self,
                        })))
                    }
                    TryLockError::WouldBlock => Err(TryLockError::WouldBlock),
                },
            }
        }
    }
    impl<'a, T, Outer: Deref<Target = Mutex<T>> + Sized + 'a> NestedMutex<'a, T> for Outer {}

    pub trait NestedRwLock<'a, T>: Deref<Target = RwLock<T>> + Sized + 'a {
        fn nest_try_read(self) -> TryLockResult<Nested<T, RwLockReadGuard<'a, T>, Self>> {
            let me = unsafe { remove_lifetime(&self) };
            match me.try_read() {
                Ok(inner) => Ok(Nested { inner, outer: self }),
                Err(err) => match err {
                    TryLockError::Poisoned(err) => {
                        let inner = err.into_inner();
                        Err(TryLockError::Poisoned(PoisonError::new(Nested {
                            inner,
                            outer: self,
                        })))
                    }
                    TryLockError::WouldBlock => Err(TryLockError::WouldBlock),
                },
            }
        }
        fn nest_try_write(self) -> TryLockResult<Nested<T, RwLockWriteGuard<'a, T>, Self>> {
            let me = unsafe { remove_lifetime(&self) };
            match me.try_write() {
                Ok(inner) => Ok(Nested { inner, outer: self }),
                Err(err) => match err {
                    TryLockError::Poisoned(err) => {
                        let inner = err.into_inner();
                        Err(TryLockError::Poisoned(PoisonError::new(Nested {
                            inner,
                            outer: self,
                        })))
                    }
                    TryLockError::WouldBlock => Err(TryLockError::WouldBlock),
                },
            }
        }
    }
    impl<'a, T, Outer: Deref<Target = RwLock<T>> + Sized + 'a> NestedRwLock<'a, T> for Outer {}

    #[cfg(test)]
    mod test {
        use super::*;

        #[test]
        fn test_many_arc() {
            let x1: Arc<i32> = Arc::new(0);
            let y1: Weak<i32> = Arc::downgrade(&x1);

            let x2: Arc<Weak<i32>> = Arc::new(y1);
            let y2: Weak<Weak<i32>> = Arc::downgrade(&x2);

            let x3: Arc<Weak<Weak<i32>>> = Arc::new(y2);
            let y3: Weak<Weak<Weak<i32>>> = Arc::downgrade(&x3);

            let z = y3
                .upgrade()
                .unwrap()
                .nest_upgrade()
                .unwrap()
                .nest_upgrade()
                .unwrap();
            assert_eq!(0, *z);
        }

        #[test]
        fn test_many_rwlock() {
            let x = RwLock::new(RwLock::new(RwLock::new(0)));
            {
                let z = x
                    .try_read()
                    .unwrap()
                    .nest_try_read()
                    .unwrap()
                    .nest_try_read()
                    .unwrap();
                assert_eq!(0, *z);
            }
            {
                let z = x
                    .try_read()
                    .unwrap()
                    .nest_try_read()
                    .unwrap()
                    .nest_try_read()
                    .unwrap();
                assert_eq!(0, *z);
            }
            {
                let mut z = x
                    .try_read()
                    .unwrap()
                    .nest_try_read()
                    .unwrap()
                    .nest_try_write()
                    .unwrap();
                *z = 1;
                assert_eq!(1, *z);
            }
            {
                {
                    let mut y = x.try_read().unwrap().nest_try_write().unwrap();
                    *y = RwLock::new(2);
                }
                let z = x
                    .try_read()
                    .unwrap()
                    .nest_try_read()
                    .unwrap()
                    .nest_try_read()
                    .unwrap();
                assert_eq!(2, *z);
            }
        }
        #[test]
        fn test_many_mutex() {
            let x = RwLock::new(RwLock::new(RwLock::new(0)));
            {
                let z = x
                    .try_read()
                    .unwrap()
                    .nest_try_read()
                    .unwrap()
                    .nest_try_read()
                    .unwrap();
                assert_eq!(0, *z);
            }
            {
                let z = x
                    .try_read()
                    .unwrap()
                    .nest_try_read()
                    .unwrap()
                    .nest_try_read()
                    .unwrap();
                assert_eq!(0, *z);
            }
            {
                let mut z = x
                    .try_read()
                    .unwrap()
                    .nest_try_read()
                    .unwrap()
                    .nest_try_write()
                    .unwrap();
                *z = 1;
                assert_eq!(1, *z);
            }
            {
                {
                    let mut y = x.try_read().unwrap().nest_try_write().unwrap();
                    *y = RwLock::new(2);
                }
                let z = x
                    .try_read()
                    .unwrap()
                    .nest_try_read()
                    .unwrap()
                    .nest_try_read()
                    .unwrap();
                assert_eq!(2, *z);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::cell::*;
    use std::mem;
    use std::rc::*;

    use super::*;

    #[test]
    fn test_rc_refcell() {
        let x = Rc::new(RefCell::new(0));
        let x = Rc::downgrade(&x);
        assert_eq!(0, *x.upgrade().unwrap().borrow());
        {
            let z = x.upgrade().unwrap().nest_borrow();
            assert_eq!(0, *z);
        }
    }

    // #[test]
    // fn test_reassign_refcell_stack_does_not_compile() {
    //   let x = RefCell::new(RefCell::new(0));
    //   let ys: Vec<_> = (1..10).map(|i|
    // RefCell::new(RefCell::new(i))).collect();

    //   let mut ref1 = x.borrow();
    //   let mut ref2 = ref1.borrow_mut();

    //   for y in ys.iter() {
    //     let mut yref1 = y.borrow();
    //     let mut yref2 = yref1.borrow_mut();
    //     mem::swap(ref2.deref_mut(), yref2.deref_mut());
    //     ref2 = yref2;
    //     ref1 = yref1;
    //   }
    // }

    #[test]
    fn test_reassign_refcell_stack() {
        let x = RefCell::new(RefCell::new(0));
        let ys: Vec<_> = (1..10).map(|i| RefCell::new(RefCell::new(i))).collect();

        let mut ref1 = x.borrow().nest_borrow_mut();

        for y in ys.iter() {
            let mut yref = y.borrow().nest_borrow_mut();
            mem::swap(ref1.deref_mut(), yref.deref_mut());
            ref1 = yref;
        }
    }
}
