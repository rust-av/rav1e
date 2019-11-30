// Copyright (c) 2017-2019, The rav1e contributors. All rights reserved
//
// This source code is subject to the terms of the BSD 2 Clause License and
// the Alliance for Open Media Patent License 1.0. If the BSD 2 Clause License
// was not distributed with this source code in the LICENSE file, you can
// obtain it at www.aomedia.org/license/software. If the Alliance for Open
// Media Patent License 1.0 was not distributed with this source code in the
// PATENTS file, you can obtain it at www.aomedia.org/license/patent.

use std::alloc::{alloc, dealloc, Layout};
use std::marker::PhantomData;
use std::mem;

use crate::util::Pixel;

/// Backing buffer for the Plane data
///
/// The buffer is padded and aligned according to the architecture-specific
/// SIMD constraints.
#[derive(Debug, PartialEq, Eq)]
pub struct PlaneData<T: Pixel> {
  ptr: std::ptr::NonNull<T>,
  _marker: PhantomData<T>,
  len: usize,
}

unsafe impl<T: Pixel + Send> Send for PlaneData<T> {}
unsafe impl<T: Pixel + Sync> Sync for PlaneData<T> {}

impl<T: Pixel> Clone for PlaneData<T> {
  fn clone(&self) -> Self {
    let mut pd = unsafe { Self::new_uninitialized(self.len) };

    pd.copy_from_slice(self);

    pd
  }
}

impl<T: Pixel> std::ops::Deref for PlaneData<T> {
  type Target = [T];

  fn deref(&self) -> &[T] {
    unsafe {
      let p = self.ptr.as_ptr();

      std::slice::from_raw_parts(p, self.len)
    }
  }
}

impl<T: Pixel> std::ops::DerefMut for PlaneData<T> {
  fn deref_mut(&mut self) -> &mut [T] {
    unsafe {
      let p = self.ptr.as_ptr();

      std::slice::from_raw_parts_mut(p, self.len)
    }
  }
}

impl<T: Pixel> std::ops::Drop for PlaneData<T> {
  fn drop(&mut self) {
    unsafe {
      dealloc(self.ptr.as_ptr() as *mut u8, Self::layout(self.len));
    }
  }
}

impl<T: Pixel> PlaneData<T> {
  /// Data alignment in bytes.
  const DATA_ALIGNMENT_LOG2: usize = 5;

  unsafe fn layout(len: usize) -> Layout {
    Layout::from_size_align_unchecked(
      len * mem::size_of::<T>(),
      1 << Self::DATA_ALIGNMENT_LOG2,
    )
  }

  unsafe fn new_uninitialized(len: usize) -> Self {
    let ptr = {
      let ptr = alloc(Self::layout(len)) as *mut T;
      std::ptr::NonNull::new_unchecked(ptr)
    };

    PlaneData { ptr, len, _marker: PhantomData }
  }

  pub fn new(len: usize) -> Self {
    let mut pd = unsafe { Self::new_uninitialized(len) };

    for v in pd.iter_mut() {
      *v = T::cast_from(128);
    }

    pd
  }

  #[cfg(any(test, feature = "bench"))]
  fn from_slice(data: &[T]) -> Self {
    let mut pd = unsafe { Self::new_uninitialized(data.len()) };

    pd.copy_from_slice(data);

    pd
  }
}
