/*! `BitSlice` Wide Reference

This module defines semantic operations on `[u1]`, in contrast to the mechanical
operations defined in `BitPtr`.

The `&BitSlice` handle has the same size and general layout as the standard Rust
slice handle `&[T]`. Its binary layout is wholly incompatible with the layout of
Rust slices, and must never be interchanged except through the provided APIs.
!*/

use crate::{
	access::BitAccess,
	cursor::{
		Cursor,
		Local,
	},
	domain::*,
	indices::Indexable,
	pointer::BitPtr,
	store::{
		BitStore,
		Word,
	},
};

#[cfg(feature = "alloc")]
use {
	alloc::borrow::ToOwned,
};

use core::{
	hash::{
		Hash,
		Hasher,
	},
	iter::FusedIterator,
	marker::PhantomData,
	ops::{
		AddAssign,
		BitAndAssign,
		BitOrAssign,
		BitXorAssign,
		Deref,
		DerefMut,
		Index,
		IndexMut,
		Neg,
		Not,
		Range,
		RangeFrom,
		RangeFull,
		RangeInclusive,
		RangeTo,
		RangeToInclusive,
		ShlAssign,
		ShrAssign,
	},
	ptr,
	str,
};

/** A compact slice of bits, whose cursor and storage types can be customized.

`BitSlice` is a specialized slice type, which can only ever be held by
reference or specialized owning pointers provided by this crate. The value
patterns of its handles are opaque binary structures, which cannot be
meaningfully inspected by user code.

`BitSlice` can only be dynamically allocated by this library. Creation of any
other `BitSlice` collections will result in severely incorrect behavior.

A `BitSlice` reference can be created through the [`bitvec!`] macro, from a
[`BitVec`] collection, or from most common Rust types (fundamentals, slices of
them, and small arrays) using the [`Bits`] and [`BitsMut`] traits.

`BitSlice`s are a view into a block of memory at bit-level resolution. They are
represented by a crate-internal pointer structure that ***cannot*** be used with
other Rust code except through the provided conversion APIs.

```rust
use bitvec::prelude::*;

# #[cfg(feature = "alloc")] {
let bv = bitvec![0, 1, 0, 1];
//  slicing a bitvec
let bslice: &BitSlice = &bv[..];
# }

//  coercing an array to a bitslice
let bslice: &BitSlice<_, _> = [1u8, 254u8].bits::<BigEndian>();
```

Bit slices are either mutable or shared. The shared slice type is
`&BitSlice<C, T>`, while the mutable slice type is `&mut BitSlice<C, T>`. For
example, you can mutate bits in the memory to which a mutable `BitSlice` points:

```rust
use bitvec::prelude::*;

let mut base = [0u8, 0, 0, 0];
{
 let bs: &mut BitSlice<_, _> = base.bits_mut::<BigEndian>();
 bs.set(13, true);
 eprintln!("{:?}", bs.as_ref());
 assert!(bs[13]);
}
assert_eq!(base[1], 4);
```

# Type Parameters

- `C`: An implementor of the `Cursor` trait. This type is used to convert
  semantic indices into concrete bit positions in elements, and store or
  retrieve bit values from the storage type.
- `T`: An implementor of the `BitStore` trait: `u8`, `u16`, `u32`, or `u64`
  (64-bit systems only). This is the actual type in memory that the slice will
  use to store data.

# Safety

The `&BitSlice` reference handle has the same *size* as standard Rust slice
handles, but it is ***extremely value-incompatible*** with them. Attempting to
treat `&BitSlice<_, T>` as `&[T]` in any manner except through the provided APIs
is ***catastrophically*** unsafe and unsound.

[`BitVec`]: ../vec/struct.BitVec.html
[`Bits`]: ../bits/trait.Bits.html
[`BitsMut`]: ../bits/trait.BitsMut.html
[`From`]: https://doc.rust-lang.org/stable/std/convert/trait.From.html
[`bitvec!`]: ../macro.bitvec.html
**/
#[repr(transparent)]
pub struct BitSlice<C = Local, T = Word>
where C: Cursor, T: BitStore {
	/// Cursor type for selecting bits inside an element.
	_kind: PhantomData<C>,
	/// Element type of the slice.
	///
	/// eddyb recommends using `PhantomData<T>` and `[()]` instead of `[T]`
	/// alone.
	_type: PhantomData<T>,
	/// Slice of elements `T` over which the `BitSlice` has usage.
	_elts: [()],
}

impl<C, T> BitSlice<C, T>
where C: Cursor, T: BitStore {
	/// Produces the empty slice. This is equivalent to `&[]` for Rust slices.
	///
	/// # Returns
	///
	/// An empty `&BitSlice` handle.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let bits: &BitSlice = BitSlice::empty();
	/// ```
	pub fn empty<'a>() -> &'a Self {
		BitPtr::empty().into_bitslice()
	}

	/// Produces the empty mutable slice. This is equivalent to `&mut []` for
	/// Rust slices.
	///
	/// # Returns
	///
	/// An empty `&mut BitSlice` handle.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let bits: &mut BitSlice = BitSlice::empty_mut();
	/// ```
	pub fn empty_mut<'a>() -> &'a mut Self {
		BitPtr::empty().into_bitslice_mut()
	}

	/// Produces an immutable `BitSlice` over a single element.
	///
	/// # Parameters
	///
	/// - `elt`: A reference to an element over which the `BitSlice` will be
	///   created.
	///
	/// # Returns
	///
	/// A `BitSlice` over the provided element.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let elt: u8 = !0;
	/// let bs: &BitSlice<Local, _> = BitSlice::from_element(&elt);
	/// assert!(bs.all());
	/// ```
	pub fn from_element(elt: &T) -> &Self {
		unsafe {
			BitPtr::new_unchecked(elt, 0u8.idx(), T::BITS as usize)
		}.into_bitslice()
	}

	/// Produces a mutable `BitSlice` over a single element.
	///
	/// # Parameters
	///
	/// - `elt`: A reference to an element over which the `BitSlice` will be
	///   created.
	///
	/// # Returns
	///
	/// A `BitSlice` over the provided element.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut elt: u8 = !0;
	/// let bs: &mut BitSlice<Local, _> = BitSlice::from_element_mut(&mut elt);
	/// bs.set(0, false);
	/// assert!(!bs.all());
	/// ```
	pub fn from_element_mut(elt: &mut T) -> &mut Self {
		unsafe {
			BitPtr::new_unchecked(elt, 0u8.idx(), T::BITS as usize)
		}.into_bitslice_mut()
	}

	/// Wraps a `&[T: BitStore]` in a `&BitSlice<C: Cursor, T>`. The cursor must
	/// be specified at the call site. The element type cannot be changed.
	///
	/// # Parameters
	///
	/// - `src`: The elements over which the new `BitSlice` will operate.
	///
	/// # Returns
	///
	/// A `BitSlice` representing the original element slice.
	///
	/// # Panics
	///
	/// The source slice must not exceed the maximum number of elements that a
	/// `BitSlice` can contain. This value is documented in [`BitPtr`].
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let src = [1, 2, 3];
	/// let bits = BitSlice::<BigEndian, u8>::from_slice(&src[..]);
	/// assert_eq!(bits.len(), 24);
	/// assert_eq!(bits.as_ref().len(), 3);
	/// assert!(bits[7]);  // src[0] == 0b0000_0001
	/// assert!(bits[14]); // src[1] == 0b0000_0010
	/// assert!(bits[22]); // src[2] == 0b0000_0011
	/// assert!(bits[23]);
	/// ```
	///
	/// [`BitPtr`]: ../pointer/struct.BitPtr.html
	pub fn from_slice(slice: &[T]) -> &Self {
		let len = slice.len();
		assert!(
			len <= BitPtr::<T>::MAX_ELTS,
			"BitSlice cannot address {} elements",
			len,
		);
		let bits = len.checked_mul(T::BITS as usize)
			.expect("Bit length out of range");
		BitPtr::new(slice.as_ptr(), 0u8.idx(), bits).into_bitslice()
	}

	/// Wraps a `&mut [T: BitStore]` in a `&mut BitSlice<C: Cursor, T>`. The
	/// cursor must be specified by the call site. The element type cannot
	/// be changed.
	///
	/// # Parameters
	///
	/// - `src`: The elements over which the new `BitSlice` will operate.
	///
	/// # Returns
	///
	/// A `BitSlice` representing the original element slice.
	///
	/// # Panics
	///
	/// The source slice must not exceed the maximum number of elements that a
	/// `BitSlice` can contain. This value is documented in [`BitPtr`].
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut src = [1, 2, 3];
	/// let bits = BitSlice::<LittleEndian, u8>::from_slice_mut(&mut src[..]);
	/// //  The first bit is the LSb of the first element.
	/// assert!(bits[0]);
	/// bits.set(0, false);
	/// assert!(!bits[0]);
	/// assert_eq!(bits.as_ref(), &[0, 2, 3]);
	/// ```
	///
	/// [`BitPtr`]: ../pointer/struct.BitPtr.html
	pub fn from_slice_mut(slice: &mut [T]) -> &mut Self {
		Self::from_slice(slice).bitptr().into_bitslice_mut()
	}

	/// Sets the bit value at the given position.
	///
	/// # Parameters
	///
	/// - `&mut self`
	/// - `index`: The bit index to set. It must be in the domain
	///   `0 .. self.len()`.
	/// - `value`: The value to be set, `true` for `1` and `false` for `0`.
	///
	/// # Panics
	///
	/// This method panics if `index` is outside the slice domain.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut store = 8u8;
	/// let bits = store.bits_mut::<BigEndian>();
	/// assert!(!bits[3]);
	/// bits.set(3, true);
	/// assert!(bits[3]);
	/// ```
	pub fn set(&mut self, index: usize, value: bool) {
		let len = self.len();
		assert!(index < len, "Index out of range: {} >= {}", index, len);
		unsafe { self.set_unchecked(index, value) };
	}

	/// Sets a bit at an index, without doing bounds checking.
	///
	/// This is generally not recommended; use with caution! For a safe
	/// alternative, see [`set`].
	///
	/// # Parameters
	///
	/// - `&mut self`
	/// - `index`: The bit index to retrieve. This index is *not* checked
	///   against the length of `self`.
	///
	/// # Effects
	///
	/// The bit at `index` is set to `value`.
	///
	/// # Safety
	///
	/// This method is **not** safe. It performs raw pointer arithmetic to seek
	/// from the start of the slice to the requested index, and set the bit
	/// there. It does not inspect the length of `self`, and it is free to
	/// perform out-of-bounds memory *write* access.
	///
	/// Use this method **only** when you have already performed the bounds
	/// check, and can guarantee that the call occurs with a safely in-bounds
	/// index.
	///
	/// # Examples
	///
	/// This example uses a bit slice of length 2, and demonstrates
	/// out-of-bounds access to the last bit in the element.
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut src = 0u8;
	/// {
	///  let bits = &mut src.bits_mut::<BigEndian>()[2 .. 4];
	///  assert_eq!(bits.len(), 2);
	///  unsafe { bits.set_unchecked(5, true); }
	/// }
	/// assert_eq!(src, 1);
	/// ```
	///
	/// [`set`]: #method.set
	pub unsafe fn set_unchecked(&mut self, index: usize, value: bool) {
		let bitptr = self.bitptr();
		let (elt, bit) = bitptr.head().offset(index as isize);
		let data_ptr = bitptr.pointer().a();
		(&*data_ptr.offset(elt)).set::<C>(bit, value);
	}

	/// Produces a write reference to a single bit in the slice.
	///
	/// The structure returned by this method extends the borrow until it drops,
	/// which precludes parallel use.
	///
	/// The [`split_at_mut`] method allows splitting the borrows of a slice, and
	/// will enable safe parallel use of these write references. The `atomic`
	/// feature guarantees that parallel use does not cause data races when
	/// modifying the underlying slice.
	///
	/// # Lifetimes
	///
	/// - `'a` Propagates the lifetime of the referent slice to the single-bit
	///   reference produced.
	///
	/// # Parameters
	///
	/// - `&mut self`
	/// - `index`: The index of the bit in `self` selected.
	///
	/// # Returns
	///
	/// A write reference to the requested bit. Due to Rust limitations, this is
	/// not a native reference type, but is a custom structure that holds the
	/// address of the requested bit and its value. The produced structure
	/// implements `Deref` and `DerefMut` to its cached bit, and commits the
	/// cached bit to the parent slice on drop.
	///
	/// # Usage
	///
	/// You must use the dereference operator on the `.at()` expression in order
	/// to assign to it. In general, you should prefer immediately using and
	/// discarding the returned value, rather than binding it to a name and
	/// letting it live for more than one statement.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut src = 0u8;
	/// let bits = src.bits_mut::<BigEndian>();
	///
	/// assert!(!bits[0]);
	/// *bits.at(0) = true;
	/// //  note the leading dereference.
	/// assert!(bits[0]);
	/// ```
	///
	/// This example shows multiple usage by using `split_at_mut`.
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut src = 0u8;
	/// let bits = src.bits_mut::<BigEndian>();
	///
	/// {
	///  let (mut a, rest) = bits.split_at_mut(2);
	///  let (mut b, rest) = rest.split_at_mut(3);
	///  *a.at(0) = true;
	///  *b.at(0) = true;
	///  *rest.at(0) = true;
	/// }
	///
	/// assert_eq!(bits.as_slice()[0], 0b1010_0100);
	/// //                               a b   rest
	/// ```
	///
	/// The above example splits the slice into three (the first, the second,
	/// and the rest) in order to hold multiple write references into the slice.
	///
	/// [`split_at_mut`]: #method.split_at_mut
	pub fn at(&mut self, index: usize) -> BitGuard<C, T> {
		let len = self.len();
		assert!(index < len, "Index {} out of bounds: {}", index, len);
		unsafe { self.at_unchecked(index) }
	}

	/// Version of [`at`](#method.at) that does not perform boundary checking.
	pub unsafe fn at_unchecked(&mut self, index: usize) -> BitGuard<C, T> {
		BitGuard {
			bit: *self.get_unchecked(index),
			slot: self.get_unchecked_mut(index ..= index),
		}
	}

	/// Version of [`split_at`](#method.split_at) that does not perform boundary
	/// checking.
	pub unsafe fn split_at_unchecked(&self, mid: usize) -> (&Self, &Self) {
		match mid {
			0 => (BitSlice::empty(), self),
			n if n == self.len() => (self, BitSlice::empty()),
			_ => (self.get_unchecked(.. mid), self.get_unchecked(mid ..)),
		}
	}

	/// Version of [`split_at_mut`](#method.split_at_mut) that does not perform
	/// boundary checking.
	pub unsafe fn split_at_mut_unchecked(
		&mut self,
		mid: usize,
	) -> (&mut Self, &mut Self) {
		let (head, tail) = self.split_at_unchecked(mid);
		(head.bitptr().into_bitslice_mut(), tail.bitptr().into_bitslice_mut())
	}

	/// Tests if *all* bits in the slice domain are set (logical `∧`).
	///
	/// # Truth Table
	///
	/// ```text
	/// 0 0 => 0
	/// 0 1 => 0
	/// 1 0 => 0
	/// 1 1 => 1
	/// ```
	///
	/// # Parameters
	///
	/// - `&self`
	///
	/// # Returns
	///
	/// Whether all bits in the slice domain are set. The empty slice returns
	/// `true`.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let bits = 0xFDu8.bits::<BigEndian>();
	/// assert!(bits[.. 4].all());
	/// assert!(!bits[4 ..].all());
	/// ```
	pub fn all(&self) -> bool {
		match self.bitptr().domain() {
			BitDomain::Empty => true,
			BitDomain::Minor(head, elt, tail) => (*head .. *tail)
				.all(|n| elt.get::<C>(n.idx())),
			BitDomain::Major(h, head, body, tail, t) => (*h .. T::BITS)
				.all(|n| head.get::<C>(n.idx()))
				&& (0 .. *t).all(|n| tail.get::<C>(n.idx()))
				&& body.iter().all(|e| e.load() == T::bits(true)),
			BitDomain::PartialHead(h, head, body) => (*h .. T::BITS)
				.all(|n| head.get::<C>(n.idx()))
				&& body.iter().all(|e| e.load() == T::bits(true)),
			BitDomain::PartialTail(body, tail, t) => (0 .. *t)
				.all(|n| tail.get::<C>(n.idx()))
				&& body.iter().all(|e| e.load() == T::bits(true)),
			BitDomain::Spanning(body) => body.iter()
				.all(|e| e.load() == T::bits(true)),
		}
	}

	/// Tests if *any* bit in the slice is set (logical `∨`).
	///
	/// # Truth Table
	///
	/// ```text
	/// 0 0 => 0
	/// 0 1 => 1
	/// 1 0 => 1
	/// 1 1 => 1
	/// ```
	///
	/// # Parameters
	///
	/// - `&self`
	///
	/// # Returns
	///
	/// Whether any bit in the slice domain is set. The empty slice returns
	/// `false`.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let bits = 0x40u8.bits::<BigEndian>();
	/// assert!(bits[.. 4].any());
	/// assert!(!bits[4 ..].any());
	/// ```
	pub fn any(&self) -> bool {
		match self.bitptr().domain() {
			BitDomain::Empty => false,
			BitDomain::Minor(head, elt, tail) => (*head .. *tail)
				.any(|n| elt.get::<C>(n.idx())),
			BitDomain::Major(h, head, body, tail, t) => (*h .. T::BITS)
				.any(|n| head.get::<C>(n.idx()))
				|| (0 .. *t).any(|n| tail.get::<C>(n.idx()))
				|| body.iter().any(|e| e.load() != T::bits(false)),
			BitDomain::PartialHead(h, head, body) => (*h .. T::BITS)
				.any(|n| head.get::<C>(n.idx()))
				|| body.iter().any(|e| e.load() != T::bits(false)),
			BitDomain::PartialTail(body, tail, t) => (0 .. *t)
				.any(|n| tail.get::<C>(n.idx()))
				|| body.iter().any(|e| e.load() != T::bits(false)),
			BitDomain::Spanning(body) => body.iter()
				.any(|e| e.load() != T::bits(false)),
		}
	}

	/// Tests if *any* bit in the slice is unset (logical `¬∧`).
	///
	/// # Truth Table
	///
	/// ```text
	/// 0 0 => 1
	/// 0 1 => 1
	/// 1 0 => 1
	/// 1 1 => 0
	/// ```
	///
	/// # Parameters
	///
	/// - `&self
	///
	/// # Returns
	///
	/// Whether any bit in the slice domain is unset.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let bits = 0xFDu8.bits::<BigEndian>();
	/// assert!(!bits[.. 4].not_all());
	/// assert!(bits[4 ..].not_all());
	/// ```
	pub fn not_all(&self) -> bool {
		!self.all()
	}

	/// Tests if *all* bits in the slice are unset (logical `¬∨`).
	///
	/// # Truth Table
	///
	/// ```text
	/// 0 0 => 1
	/// 0 1 => 0
	/// 1 0 => 0
	/// 1 1 => 0
	/// ```
	///
	/// # Parameters
	///
	/// - `&self`
	///
	/// # Returns
	///
	/// Whether all bits in the slice domain are unset.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let bits = 0x40u8.bits::<BigEndian>();
	/// assert!(!bits[.. 4].not_any());
	/// assert!(bits[4 ..].not_any());
	/// ```
	pub fn not_any(&self) -> bool {
		!self.any()
	}

	/// Tests whether the slice has some, but not all, bits set and some, but
	/// not all, bits unset.
	///
	/// This is `false` if either `all()` or `not_any()` are `true`.
	///
	/// # Truth Table
	///
	/// ```text
	/// 0 0 => 0
	/// 0 1 => 1
	/// 1 0 => 1
	/// 1 1 => 0
	/// ```
	///
	/// # Parameters
	///
	/// - `&self`
	///
	/// # Returns
	///
	/// Whether the slice domain has mixed content. The empty slice returns
	/// `false`.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let bits = 0b111_000_10u8.bits::<BigEndian>();
	/// assert!(!bits[0 .. 3].some());
	/// assert!(!bits[3 .. 6].some());
	/// assert!(bits[6 ..].some());
	/// ```
	pub fn some(&self) -> bool {
		self.any() && self.not_all()
	}

	/// Counts how many bits are set high.
	///
	/// # Parameters
	///
	/// - `&self`
	///
	/// # Returns
	///
	/// The number of high bits in the slice domain.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let bits = [0xFDu8, 0x25].bits::<BigEndian>();
	/// assert_eq!(bits.count_ones(), 10);
	/// ```
	pub fn count_ones(&self) -> usize {
		match self.bitptr().domain() {
			BitDomain::Empty => 0,
			BitDomain::Minor(head, elt, tail) => (*head .. *tail)
				.map(|n| elt.get::<C>(n.idx()) as usize)
				.sum(),
			BitDomain::Major(h, head, body, tail, t) => (*h .. T::BITS)
				.map(|n| head.get::<C>(n.idx()) as usize)
				.sum::<usize>()
				+ body.iter()
					.map(BitAccess::<T>::load)
					.map(T::count_ones)
					.sum::<usize>()
				+ (0 .. *t)
					.map(|n| tail.get::<C>(n.idx()) as usize)
					.sum::<usize>(),
			BitDomain::PartialHead(h, head, body) => (*h .. T::BITS)
				.map(|n| head.get::<C>(n.idx()) as usize)
				.sum::<usize>()
				+ body.iter()
					.map(BitAccess::<T>::load)
					.map(T::count_ones)
					.sum::<usize>(),
			BitDomain::PartialTail(body, tail, t) => body.iter()
				.map(BitAccess::<T>::load)
				.map(T::count_ones)
				.sum::<usize>()
				+ (0 .. *t)
					.map(|n| tail.get::<C>(n.idx()) as usize)
					.sum::<usize>(),
			BitDomain::Spanning(body) => body.iter()
				.map(BitAccess::<T>::load)
				.map(T::count_ones)
				.sum(),
		}
	}

	/// Counts how many bits are set low.
	///
	/// # Parameters
	///
	/// - `&self`
	///
	/// # Returns
	///
	/// The number of low bits in the slice domain.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let bits = [0xFDu8, 0x25].bits::<BigEndian>();
	/// assert_eq!(bits.count_zeros(), 6);
	/// ```
	pub fn count_zeros(&self) -> usize {
		match self.bitptr().domain() {
			BitDomain::Empty => 0,
			BitDomain::Minor(head, elt, tail) => (*head .. *tail)
				.map(|n| !elt.get::<C>(n.idx()) as usize)
				.sum(),
			BitDomain::Major(h, head, body, tail, t) => (*h .. T::BITS)
				.map(|n| !head.get::<C>(n.idx()) as usize)
				.sum::<usize>()
				+ body.iter()
					.map(BitAccess::<T>::load)
					.map(T::count_zeros)
					.sum::<usize>()
				+ (0 .. *t)
					.map(|n| !tail.get::<C>(n.idx()) as usize)
					.sum::<usize>(),
			BitDomain::PartialHead(h, head, body) => (*h .. T::BITS)
				.map(|n| !head.get::<C>(n.idx()) as usize)
				.sum::<usize>()
				+ body.iter()
					.map(BitAccess::<T>::load)
					.map(T::count_zeros)
					.sum::<usize>(),
			BitDomain::PartialTail(body, tail, t) => body.iter()
				.map(BitAccess::<T>::load)
				.map(T::count_zeros)
				.sum::<usize>()
				+ (0 .. *t)
					.map(|n| !tail.get::<C>(n.idx()) as usize)
					.sum::<usize>(),
			BitDomain::Spanning(body) => body.iter()
				.map(BitAccess::<T>::load)
				.map(T::count_zeros)
				.sum(),
		}
	}

	/// Set all bits in the slice to a value.
	///
	/// # Parameters
	///
	/// - `&mut self`
	/// - `value`: The bit value to which all bits in the slice will be set.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut src = 0u8;
	/// let bits = src.bits_mut::<BigEndian>();
	/// bits[2 .. 6].set_all(true);
	/// assert_eq!(bits.as_ref(), &[0b0011_1100]);
	/// bits[3 .. 5].set_all(false);
	/// assert_eq!(bits.as_ref(), &[0b0010_0100]);
	/// bits[.. 1].set_all(true);
	/// assert_eq!(bits.as_ref(), &[0b1010_0100]);
	/// ```
	pub fn set_all(&mut self, value: bool) {
		match self.bitptr().domain() {
			BitDomain::Empty => {},
			//  Generalizing `BitField` over any cursor would allow these
			//  accesses to become parallel rather than sequential.
			BitDomain::Minor(head, elt, tail) => (*head .. *tail)
				.for_each(|n| elt.set::<C>(n.idx(), value)),
			BitDomain::Major(h, head, body, tail, t) => {
				(*h .. T::BITS).for_each(|n| head.set::<C>(n.idx(), value));
				body.iter().for_each(|elt| elt.store(T::bits(value)));
				(0 .. *t).for_each(|n| tail.set::<C>(n.idx(), value));
			},
			BitDomain::PartialHead(h, head, body) => {
				(*h .. T::BITS).for_each(|n| head.set::<C>(n.idx(), value));
				body.iter().for_each(|elt| elt.store(T::bits(value)));
			},
			BitDomain::PartialTail(body, tail, t) => {
				body.iter().for_each(|elt| elt.store(T::bits(value)));
				(0 .. *t).for_each(|n| tail.set::<C>(n.idx(), value));
			},
			BitDomain::Spanning(body) => body.iter()
				.for_each(|elt| elt.store(T::bits(value))),
		}
	}

	/// Provides mutable traversal of the collection.
	///
	/// It is impossible to implement `IndexMut` on `BitSlice`, because bits do
	/// not have addresses, so there can be no `&mut u1`. This method allows the
	/// client to receive an enumerated bit, and provide a new bit to set at
	/// each index.
	///
	/// # Parameters
	///
	/// - `&mut self`
	/// - `func`: A function which receives a `(usize, bool)` pair of index and
	///   value, and returns a bool. It receives the bit at each position, and
	///   the return value is written back at that position.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut src = 0u8;
	/// {
	///  let bits = src.bits_mut::<BigEndian>();
	///  bits.for_each(|idx, _bit| idx % 3 == 0);
	/// }
	/// assert_eq!(src, 0b1001_0010);
	/// ```
	pub fn for_each<F>(&mut self, func: F)
	where F: Fn(usize, bool) -> bool {
		for idx in 0 .. self.len() {
			let tmp = unsafe { *self.get_unchecked(idx) };
			let new = func(idx, tmp);
			unsafe { self.set_unchecked(idx, new); }
		}
	}

	/// Performs “reverse” addition (left to right instead of right to left).
	///
	/// This addition interprets the slice, and the other addend, as having its
	/// least significant bits first in the order and its most significant bits
	/// last. This is most likely to be numerically useful under a
	/// `LittleEndian` `Cursor` type.
	///
	/// # Parameters
	///
	/// - `&mut self`: The addition uses `self` as one addend, and writes the
	///   sum back into `self`.
	/// - `addend: impl IntoIterator<Item=bool>`: A stream of bits. When this is
	///   another `BitSlice`, iteration proceeds from left to right.
	///
	/// # Return
	///
	/// The final carry bit is returned
	///
	/// # Effects
	///
	/// Starting from index `0` and proceeding upwards until either `self` or
	/// `addend` expires, the carry-propagated addition of `self[i]` and
	/// `addend[i]` is written to `self[i]`.
	///
	/// ```text
	///   101111
	/// + 0010__ (the two missing bits are logically zero)
	/// --------
	///   100000 1 (the carry-out is returned)
	/// ```
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut a = 0b0000_1010u8;
	/// let     b = 0b0000_1100u8;
	/// //      s =      1 0110
	/// let ab = &mut a.bits_mut::<LittleEndian>()[.. 4];
	/// let bb = &    b.bits::<LittleEndian>()[.. 4];
	/// let c = ab.add_assign_reverse(bb.iter().copied());
	/// assert!(c);
	/// assert_eq!(a, 0b0000_0110u8);
	/// ```
	///
	/// # Performance Notes
	///
	/// When using `LittleEndian` `Cursor` types, this can be accelerated by
	/// delegating the addition to the underlying types. This is a software
	/// implementation of the [ripple-carry adder], which has `O(n)` runtime in
	/// the number of bits. The CPU is much faster, as it has access to
	/// element-wise or vectorized addition operations.
	///
	/// If your use case sincerely needs binary-integer arithmetic operations on
	/// bit sets
	///
	/// [ripple-carry adder]: https://en.wikipedia.org/wiki/Ripple-carry_adder
	pub fn add_assign_reverse<I>(&mut self, addend: I) -> bool
	where I: IntoIterator<Item=bool> {
		//  See AddAssign::add_assign for algorithm details
		let mut c = false;
		let len = self.len();
		let zero = core::iter::repeat(false);
		for (i, b) in addend.into_iter().chain(zero).enumerate().take(len) {
			//  The iterator is clamped to the upper bound of `self`.
			let a = unsafe { *self.get_unchecked(i) };
			let (y, z) = crate::rca1(a, b, c);
			//  Write the sum into `self`
			unsafe { self.set_unchecked(i, y); }
			//  Propagate the carry
			c = z;
		}
		c
	}

	/// Accesses the backing storage of the `BitSlice` as a slice of its
	/// elements.
	///
	/// This will not include partially-owned edge elements, as they may be
	/// contended by other slice handles.
	///
	/// # Parameters
	///
	/// - `&self`
	///
	/// # Returns
	///
	/// A slice of all the elements that the `BitSlice` uses for storage.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let src = [1u8, 66];
	/// let bits = src.bits::<BigEndian>();
	///
	/// let accum = bits.as_slice()
	///   .iter()
	///   .map(|elt| elt.count_ones())
	///   .sum::<u32>();
	/// assert_eq!(accum, 3);
	/// ```
	pub fn as_slice(&self) -> &[T] {
		&* unsafe { BitAccess::as_slice_mut(match self.bitptr().domain() {
			| BitDomain::Empty
			| BitDomain::Minor(_, _, _) => &[],
			| BitDomain::PartialHead(_, _, body)
			| BitDomain::PartialTail(body, _, _)
			| BitDomain::Major(_, _, body, _, _)
			| BitDomain::Spanning(body) => body,
		}) }
	}

	/// Accesses the underlying store.
	///
	/// This will not include partially-owned edge elements, as they may be
	/// contended by other slice handles.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut src = [1u8, 64];
	/// let bits = src.bits_mut::<BigEndian>();
	/// for elt in bits.as_mut_slice() {
	///   *elt |= 2;
	/// }
	/// assert_eq!(&[3, 66], bits.as_slice());
	/// ```
	pub fn as_mut_slice(&mut self) -> &mut [T] {
		unsafe { BitAccess::as_slice_mut(match self.bitptr().domain() {
			| BitDomain::Empty
			| BitDomain::Minor(_, _, _) => &[],
			| BitDomain::PartialHead(_, _, body)
			| BitDomain::PartialTail(body, _, _)
			| BitDomain::Major(_, _, body, _, _)
			| BitDomain::Spanning(body) => body,
		}) }
	}

	/// Accesses the underlying store, including contended partial elements.
	///
	/// This produces a slice of element wrappers that permit shared mutation,
	/// rather than a slice of the bare `T` fundamentals.
	///
	/// # Parameters
	///
	/// - `&self`
	///
	/// # Returns
	///
	/// A slice of all elements under the bit span, including any
	/// partially-owned edge elements, wrapped in safe shared-mutation types.
	pub fn as_total_slice(&self) -> &[T::Access] {
		self.bitptr().as_access_slice()
	}

	/// Accesses the underlying pointer structure.
	///
	/// # Parameters
	///
	/// - `&self`
	///
	/// # Returns
	///
	/// The [`BitPtr`] structure of the slice handle.
	///
	/// [`BitPtr`]: ../pointer/struct.BitPtr.html
	pub(crate) fn bitptr(&self) -> BitPtr<T> {
		BitPtr::from_bitslice(self)
	}

	/// Copy a bit from one location in a slice to another.
	///
	/// # Parameters
	///
	/// - `&mut self`
	/// - `from`: The index of the bit to be copied.
	/// - `to`: The index at which the copied bit will be written.
	///
	/// # Safety
	///
	/// `from` and `to` must be within the bounds of `self`. This is not
	/// checked.
	unsafe fn copy_unchecked(&mut self, from: usize, to: usize) {
		self.set_unchecked(to, *self.get_unchecked(from));
	}
}

/// Writes the contents of the `BitSlice`, in semantic bit order, into a hasher.
impl<C, T> Hash for BitSlice<C, T>
where C: Cursor, T: BitStore {
	/// Writes each bit of the `BitSlice`, as a full `bool`, into the hasher.
	///
	/// # Parameters
	///
	/// - `&self`
	/// - `hasher`: The hashing state into which the slice will be written.
	///
	/// # Type Parameters
	///
	/// - `H: Hasher`: The type of the hashing algorithm which receives the bits
	///   of `self`.
	fn hash<H>(&self, hasher: &mut H)
	where H: Hasher {
		for bit in self {
			hasher.write_u8(*bit as u8);
		}
	}
}

/** `BitSlice` is safe to move across thread boundaries, when atomic operations
are enabled.

Consider this (contrived) example:

```rust
# #[cfg(feature = "std")] {
use bitvec::prelude::*;
use std::thread;

static mut SRC: u8 = 0;
# {
let bits = unsafe { SRC.bits_mut::<BigEndian>() };
let (l, r) = bits.split_at_mut(4);

let a = thread::spawn(move || l.set(2, true));
let b = thread::spawn(move || r.set(2, true));
a.join();
b.join();
# }

println!("{:02X}", unsafe { SRC });
# }
```

Without atomic operations, this is logically a data race. It *so happens* that,
on x86, the read/modify/write cycles used in the crate are *basically* atomic by
default, even when not specified as such. This is not necessarily true on other
architectures, however.
**/
#[cfg(feature = "atomic")]
unsafe impl<C, T> Send for BitSlice<C, T>
where C: Cursor, T: BitStore {}

/** Reading across threads still has synchronization concerns if one thread can
mutate, so read access across threads requires atomicity in order to ensure that
write operations from one thread to an element conclude before another thread
can read from the element, even if the two `BitSlice`s do not collide.
**/
#[cfg(feature = "atomic")]
unsafe impl<C, T> Sync for BitSlice<C, T>
where C: Cursor, T: BitStore {}

/** Performs unsigned addition in place on a `BitSlice`.

If the addend bitstream is shorter than `self`, the addend is zero-extended at
the left (so that its final bit matches with `self`’s final bit). If the addend
is longer, the excess front length is unused.

Addition proceeds from the right ends of each slice towards the left. Because
this trait is forbidden from returning anything, the final carry-out bit is
discarded.

Note that, unlike `BitVec`, there is no subtraction implementation until I find
a subtraction algorithm that does not require modifying the subtrahend.

Subtraction can be implemented by negating the intended subtrahend yourself and
then using addition, or by using `BitVec`s instead of `BitSlice`s.

# Type Parameters

- `I: IntoIterator<Item=bool, IntoIter: DoubleEndedIterator>`: The bitstream to
  add into `self`. It must be finite and double-ended, since addition operates
  in reverse.
**/
impl<C, T, I> AddAssign<I> for BitSlice<C, T>
where C: Cursor, T: BitStore,
	I: IntoIterator<Item=bool>, I::IntoIter: DoubleEndedIterator {
	/// Performs unsigned wrapping addition in place.
	///
	/// # Examples
	///
	/// This example shows addition of a slice wrapping from max to zero.
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut src = [0b1110_1111u8, 0b0000_0001];
	/// let bits = src.bits_mut::<BigEndian>();
	/// let (nums, one) = bits.split_at_mut(12);
	/// let (accum, steps) = nums.split_at_mut(4);
	/// *accum += one.iter().copied();
	/// assert_eq!(accum, &steps[.. 4]);
	/// *accum += one.iter().copied();
	/// assert_eq!(accum, &steps[4 ..]);
	/// ```
	//  Clippy doesn’t like single-letter names (which is accurate) but this is
	//  pretty standard mathematical notation in EE.
	#[allow(clippy::many_single_char_names)]
	fn add_assign(&mut self, addend: I) {
		use core::iter::repeat;

		//  I don't, at this time, want to implement a carry-lookahead adder in
		//  software, so this is going to be a plain ripple-carry adder with
		//  O(n) runtime. Furthermore, until I think of an optimization
		//  strategy, it is going to build up another bitvec to use as a stack.
		//
		//  Computers are fast. Whatever.
		let mut c = false;
		//  Reverse self, reverse addend and zero-extend, and zip both together.
		//  This walks both slices from rightmost to leftmost, and considers an
		//  early expiration of addend to continue with 0 bits.
		//
		//  100111
		// +  0010
		//  ^^---- semantically zero
		let addend_iter = addend.into_iter().rev().chain(repeat(false));
		for (i, b) in (0 .. self.len()).rev().zip(addend_iter) {
			//  Bounds checks are performed in the loop header.
			let a = unsafe { *self.get_unchecked(i) };
			let (y, z) = crate::rca1(a, b, c);
			unsafe { self.set_unchecked(i, y); }
			c = z;
		}
	}
}

/** Performs the Boolean `AND` operation against another bitstream and writes
the result into `self`. If the other bitstream ends before `self,`, the
remaining bits of `self` are cleared.

# Type Parameters

- `I: IntoIterator<Item=bool>`: A stream of bits, which may be a `BitSlice`
  or some other bit producer as desired.
**/
impl<C, T, I> BitAndAssign<I> for BitSlice<C, T>
where C: Cursor, T: BitStore, I: IntoIterator<Item=bool> {
	/// `AND`s a bitstream into a slice.
	///
	/// # Parameters
	///
	/// - `&mut self`
	/// - `rhs`: The bitstream to `AND` into `self`.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut store = [0b0101_0100u8];
	/// let     other = [0b0011_0000u8];
	/// let lhs = store.bits_mut::<BigEndian>();
	/// let rhs = other.bits::<BigEndian>();
	/// lhs[.. 6] &= rhs[.. 4].iter().copied();
	/// assert_eq!(store[0], 0b0001_0000);
	/// ```
	fn bitand_assign(&mut self, rhs: I) {
		use core::iter;
		rhs.into_iter()
			.chain(iter::repeat(false))
			.enumerate()
			.take(self.len())
			.for_each(|(idx, bit)| unsafe {
				let val = *self.get_unchecked(idx);
				self.set_unchecked(idx, val & bit);
			});
	}
}

/** Performs the Boolean `OR` operation against another bitstream and writes the
result into `self`. If the other bitstream ends before `self`, the remaining
bits of `self` are not affected.

# Type Parameters

- `I: IntoIterator<Item=bool>`: A stream of bits, which may be a `BitSlice`
  or some other bit producer as desired.
**/
impl<C, T, I> BitOrAssign<I> for BitSlice<C, T>
where C: Cursor, T: BitStore, I: IntoIterator<Item=bool> {
	/// `OR`s a bitstream into a slice.
	///
	/// # Parameters
	///
	/// - `&mut self`
	/// - `rhs`: The bitstream to `OR` into `self`.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut store = [0b0101_0100u8];
	/// let     other = [0b0011_0000u8];
	/// let lhs = store.bits_mut::<BigEndian>();
	/// let rhs = other.bits::<BigEndian>();
	/// lhs[.. 6] |= rhs[.. 4].iter().copied();
	/// assert_eq!(store[0], 0b0111_0100);
	/// ```
	fn bitor_assign(&mut self, rhs: I) {
		rhs.into_iter()
			.enumerate()
			.take(self.len())
			.for_each(|(idx, bit)| unsafe {
				let val = *self.get_unchecked(idx);
				self.set_unchecked(idx, val | bit);
			});
	}
}

/** Performs the Boolean `XOR` operation against another bitstream and writes
the result into `self`. If the other bitstream ends before `self`, the remaining
bits of `self` are not affected.

# Type Parameters

- `I: IntoIterator<Item=bool>`: A stream of bits, which may be a `BitSlice`
  or some other bit producer as desired.
**/
impl<C, T, I> BitXorAssign<I> for BitSlice<C, T>
where C: Cursor, T: BitStore, I: IntoIterator<Item=bool> {
	/// `XOR`s a bitstream into a slice.
	///
	/// # Parameters
	///
	/// - `&mut self`
	/// - `rhs`: The bitstream to `XOR` into `self`.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut store = [0b0101_0100u8];
	/// let     other = [0b0011_0000u8];
	/// let lhs = store.bits_mut::<BigEndian>();
	/// let rhs = other.bits::<BigEndian>();
	/// lhs[.. 6] ^= rhs[.. 4].iter().copied();
	/// assert_eq!(store[0], 0b0110_0100);
	/// ```
	fn bitxor_assign(&mut self, rhs: I) {
		rhs.into_iter()
			.enumerate()
			.take(self.len())
			.for_each(|(idx, bit)| unsafe {
				let val = *self.get_unchecked(idx);
				self.set_unchecked(idx, val ^ bit);
			})
	}
}

/// Indexes a single bit by semantic count. The index must be less than the
/// length of the `BitSlice`.
impl<C, T> Index<usize> for BitSlice<C, T>
where C: Cursor, T: BitStore {
	type Output = bool;

	/// Looks up a single bit by semantic index.
	///
	/// # Parameters
	///
	/// - `&self`
	/// - `index`: The semantic index of the bit to look up.
	///
	/// # Returns
	///
	/// The value of the bit at the requested index.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let src = 0b0010_0000u8;
	/// let bits = src.bits::<BigEndian>();
	/// assert!(bits[2]);
	/// assert!(!bits[3]);
	/// ```
	fn index(&self, index: usize) -> &Self::Output {
		let len = self.len();
		assert!(index < len, "Index out of range: {} >= {}", index, len);
		if unsafe { *self.get_unchecked(index) } { &true } else { &false }
	}
}

impl<C, T> Index<Range<usize>> for BitSlice<C, T>
where C: Cursor, T: BitStore {
	type Output = Self;

	fn index(&self, Range { start, end }: Range<usize>) -> &Self::Output {
		let (data, head, len) = self.bitptr().raw_parts();
		assert!(
			start <= len,
			"Index {} out of range: {}",
			start,
			len,
		);
		assert!(end <= len, "Index {} out of range: {}", end, len);
		assert!(start <= end, "Ranges can only run from low to high");
		//  Find the number of elements to drop from the front, and the index of
		//  the new head
		let (skip, new_head) = head.offset(start as isize);
		let new_len = end - start;
		unsafe { BitPtr::new_unchecked(
			data.r().offset(skip),
			new_head,
			new_len,
		) }.into_bitslice::<C>()
	}
}

impl<C, T> IndexMut<Range<usize>> for BitSlice<C, T>
where C: Cursor, T: BitStore {
	fn index_mut(
		&mut self,
		Range { start, end }: Range<usize>,
	) -> &mut Self::Output {
		//  Get an immutable slice, and then type-hack mutability back in.
		(&self[start .. end]).bitptr().into_bitslice_mut()
	}
}

impl<C, T> Index<RangeInclusive<usize>> for BitSlice<C, T>
where C: Cursor, T: BitStore {
	type Output = Self;

	fn index(&self, index: RangeInclusive<usize>) -> &Self::Output {
		let start = *index.start();
		//  This check can never fail, due to implementation details of
		//  `BitPtr<T>`.
		if let Some(end) = index.end().checked_add(1) {
			&self[start .. end]
		}
		else {
			&self[start ..]
		}
	}
}

impl<C, T> IndexMut<RangeInclusive<usize>> for BitSlice<C, T>
where C: Cursor, T: BitStore {
	fn index_mut(&mut self, index: RangeInclusive<usize>) -> &mut Self::Output {
		let start = *index.start();
		//  This check can never fail, due to implementation details of
		//  `BitPtr<T>`.
		if let Some(end) = index.end().checked_add(1) {
			&mut self[start .. end]
		}
		else {
			&mut self[start ..]
		}
	}
}

impl<C, T> Index<RangeFrom<usize>> for BitSlice<C, T>
where C: Cursor, T: BitStore {
	type Output = Self;

	fn index(&self, RangeFrom { start }: RangeFrom<usize>) -> &Self::Output {
		&self[start .. self.len()]
	}
}

impl<C, T> IndexMut<RangeFrom<usize>> for BitSlice<C, T>
where C: Cursor, T: BitStore {
	fn index_mut(
		&mut self,
		RangeFrom { start }: RangeFrom<usize>,
	) -> &mut Self::Output {
		let len = self.len();
		&mut self[start .. len]
	}
}

impl<C, T> Index<RangeFull> for BitSlice<C, T>
where C: Cursor, T: BitStore {
	type Output = Self;

	fn index(&self, _: RangeFull) -> &Self::Output {
		self
	}
}

impl<C, T> IndexMut<RangeFull> for BitSlice<C, T>
where C: Cursor, T: BitStore {
	fn index_mut(&mut self, _: RangeFull) -> &mut Self::Output {
		self
	}
}

impl<C, T> Index<RangeTo<usize>> for BitSlice<C, T>
where C: Cursor, T: BitStore {
	type Output = Self;

	fn index(&self, RangeTo { end }: RangeTo<usize>) -> &Self::Output {
		&self[0 .. end]
	}
}

impl<C, T> IndexMut<RangeTo<usize>> for BitSlice<C, T>
where C: Cursor, T: BitStore {
	fn index_mut(
		&mut self,
		RangeTo { end }: RangeTo<usize>,
	) -> &mut Self::Output {
		&mut self[0 .. end]
	}
}

impl<C, T> Index<RangeToInclusive<usize>> for BitSlice<C, T>
where C: Cursor, T: BitStore {
	type Output = Self;

	fn index(
		&self,
		RangeToInclusive { end }: RangeToInclusive<usize>,
	) -> &Self::Output {
		&self[0 ..= end]
	}
}

impl<C, T> IndexMut<RangeToInclusive<usize>> for BitSlice<C, T>
where C: Cursor, T: BitStore {
	fn index_mut(
		&mut self,
		RangeToInclusive { end }: RangeToInclusive<usize>,
	) -> &mut Self::Output {
		&mut self[0 ..= end]
	}
}

/** Performs fixed-width 2’s-complement negation of a `BitSlice`.

Unlike the `!` operator (`Not` trait), the unary `-` operator treats the
`BitSlice` as if it represents a signed 2’s-complement integer of fixed
width. The negation of a number in 2’s complement is defined as its
inversion (using `!`) plus one, and on fixed-width numbers has the following
discontinuities:

- A slice whose bits are all zero is considered to represent the number zero
  which negates as itself.
- A slice whose bits are all one is considered to represent the most negative
  number, which has no correpsonding positive number, and thus negates as zero.

This behavior was chosen so that all possible values would have *some*
output, and so that repeated application converges at idempotence. The most
negative input can never be reached by negation, but `--MOST_NEG` converges
at the least unreasonable fallback value, 0.

Because `BitSlice` cannot move, the negation is performed in place.
**/
impl<'a, C, T> Neg for &'a mut BitSlice<C, T>
where C: Cursor, T: 'a + BitStore {
	type Output = Self;

	/// Perform 2’s-complement fixed-width negation.
	///
	/// Negation is accomplished by inverting the bits and adding one. This has
	/// one edge case: `1000…`, the most negative number for its width, will
	/// negate to zero instead of itself. It thas no corresponding positive
	/// number to which it can negate.
	///
	/// # Parameters
	///
	/// - `self`
	///
	/// # Examples
	///
	/// The contortions shown here are a result of this operator applying to a
	/// mutable reference, and this example balancing access to the original
	/// `BitVec` for comparison with aquiring a mutable borrow *as a slice* to
	/// ensure that the `BitSlice` implementation is used, not the `BitVec`.
	///
	/// Negate an arbitrary positive number (first bit unset).
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut src = 0b0110_1010u8;
	/// let bits = src.bits_mut::<BigEndian>();
	/// eprintln!("{:?}", bits.split_at(4));
	/// let num = &mut bits[.. 4];
	/// -num;
	/// eprintln!("{:?}", bits.split_at(4));
	/// assert_eq!(&bits[.. 4], &bits[4 ..]);
	/// ```
	///
	/// Negate an arbitrary negative number. This example will use the above
	/// result to demonstrate round-trip correctness.
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut src = 0b1010_0110u8;
	/// let bits = src.bits_mut::<BigEndian>();
	/// let num = &mut bits[.. 4];
	/// -num;
	/// assert_eq!(&bits[.. 4], &bits[4 ..]);
	/// ```
	///
	/// Negate the most negative number, which will become zero, and show
	/// convergence at zero.
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut src = 128u8;
	/// let bits = src.bits_mut::<BigEndian>();
	/// let num = &mut bits[..];
	/// -num;
	/// assert!(bits.not_any());
	/// let num = &mut bits[..];
	/// -num;
	/// assert!(bits.not_any());
	/// ```
	fn neg(self) -> Self::Output {
		//  negative zero is zero. The invert-and-add will result in zero, but
		//  this case can be detected quickly.
		if self.is_empty() || self.not_any() {
			return self;
		}
		//  The most negative number (leading one, all zeroes else) negates to
		//  zero.
		if unsafe { *self.get_unchecked(0) } {
			//  Testing the whole range, rather than [1 ..], is more likely to
			//  hit the fast path for `not_any`.
			unsafe { self.set_unchecked(0, false); }
			if self.not_any() {
				return self;
			}
			unsafe { self.set_unchecked(0, true); }
		}
		let this = !self;
		*this += core::iter::once(true);
		this
	}
}

/// Flips all bits in the slice, in place.
impl<'a, C, T> Not for &'a mut BitSlice<C, T>
where C: Cursor, T: 'a + BitStore {
	type Output = Self;

	/// Inverts all bits in the slice.
	///
	/// This will not affect bits outside the slice in slice storage elements.
	///
	/// # Parameters
	///
	/// - `self`
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut src = [0u8; 2];
	/// let bits = &mut src.bits_mut::<BigEndian>()[2 .. 14];
	/// let _ = !bits;
	/// //  The `bits` binding is consumed by the `!` operator, and a new
	/// //  reference is returned.
	/// // assert_eq!(bits.as_ref(), &[!0, !0]);
	/// assert_eq!(src, [0x3F, 0xFC]);
	/// ```
	fn not(self) -> Self::Output {
		match self.bitptr().domain() {
			BitDomain::Empty => {},
			BitDomain::Minor(head, elt, tail) => (*head .. *tail)
				.for_each(|n| elt.invert_bit::<C>(n.idx())),
			BitDomain::Major(h, head, body, tail, t) => {
				(*h .. T::BITS).for_each(|n| head.invert_bit::<C>(n.idx()));
				body.iter().for_each(|elt| elt.store(!elt.load()));
				(0 .. *t).for_each(|n| tail.invert_bit::<C>(n.idx()));
			},
			BitDomain::PartialHead(h, head, body) => {
				(*h .. T::BITS).for_each(|n| head.invert_bit::<C>(n.idx()));
				body.iter().for_each(|elt| elt.store(!elt.load()));
			},
			BitDomain::PartialTail(body, tail, t) => {
				body.iter().for_each(|elt| elt.store(!elt.load()));
				(0 .. *t).for_each(|n| tail.invert_bit::<C>(n.idx()));
			},
			BitDomain::Spanning(body) => body.iter()
				.for_each(|elt| elt.store(!elt.load())),
		}
		self
	}
}

__bitslice_shift!(u8, u16, u32, u64, i8, i16, i32, i64);

/** Shifts all bits in the array to the left — **DOWN AND TOWARDS THE FRONT**.

On fundamentals, the left-shift operator `<<` moves bits away from the origin
and  towards the ceiling. This is because we label the bits in a primitive with
the  minimum on the right and the maximum on the left, which is big-endian bit
order.  This increases the value of the primitive being shifted.

**THAT IS NOT HOW `BitSlice` WORKS!**

`BitSlice` defines its layout with the minimum on the left and the maximum on
the right! Thus, left-shifting moves bits towards the **minimum**.

In BigEndian order, the effect in memory will be what you expect the `<<`
operator to do.

**In LittleEndian order, the effect will be equivalent to using `>>` on the**
**fundamentals in memory!**

# Notes

In order to preserve the effecs in memory that this operator traditionally
expects, the bits that are emptied by this operation are zeroed rather than
left to their old value.

The shift amount is modulated against the array length, so it is not an
error to pass a shift amount greater than the array length.

A shift amount of zero is a no-op, and returns immediately.
**/
impl<C, T> ShlAssign<usize> for BitSlice<C, T>
where C: Cursor, T: BitStore {
	/// Shifts a slice left, in place.
	///
	/// # Parameters
	///
	/// - `&mut self`
	/// - `shamt`: The shift amount. If this is greater than the length, then
	///   the slice is zeroed immediately.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut src = [0x4Bu8, 0xA5];
	/// let bits = &mut src.bits_mut::<BigEndian>()[2 .. 14];
	/// *bits <<= 3;
	/// assert_eq!(src, [0b01_011_101, 0b001_000_01]);
	/// ```
	fn shl_assign(&mut self, shamt: usize) {
		if shamt == 0 {
			return;
		}
		let len = self.len();
		if shamt >= len {
			self.set_all(false);
			return;
		}
		//  If the slice fully owns its memory, then a fast path is available
		//  with element-wise `memmove`.
		if self.bitptr().domain().is_spanning() {
			//  Compute the shift distance measured in elements.
			let offset = shamt >> T::INDX;
			//  Compute the number of elements that will remain.
			let rem = self.bitptr().elements() - offset;

			/* Memory model: suppose we have this slice of sixteen elements,
			that is shifted five elements to the left. We have three pointers
			and two lengths to manage.
			- rem is 11 (len - offset)
			- offset is 5
			- to is &[0 .. 11]
			- from is &[5 .. 16]
			- tail is &[11]
			  _ _ _ _ _ v-------before------v
			[ 0 1 2 3 4 5 6 7 8 9 a b c d e f ]
			  ^-------after-------^ 0 0 0 0 0
			*/

			//  Pointer to the front of the slice.
			let to: *mut T = self.as_mut_ptr();
			//  Pointer to the front of the section that will move and be
			//  retained.
			let from: *const T = &self.as_slice()[offset];
			//  Pointer to the back of the slice that will be zero-filled.
			let tail: *mut T = &mut self.as_mut_slice()[rem];
			unsafe {
				ptr::copy(from, to, rem);
				ptr::write_bytes(tail, 0, offset);
			}
			//  Any remaining shift amount only needs to shift the `after` block
			//  above.
			self[.. rem << T::INDX] <<= shamt & T::INDX as usize;
			return;
		}
		//  Otherwise, crawl.
		for (to, from) in (shamt .. len).enumerate() {
			unsafe { self.copy_unchecked(from, to); }
		}
		self[len - shamt ..].set_all(false);
	}
}

/** Shifts all bits in the array to the right — **UP AND TOWARDS THE BACK**.

On fundamentals, the right-shift operator `>>` moves bits towards the origin and
away from the ceiling. This is because we label the bits in a primitive with the
minimum on the right and the maximum on the left, which is big-endian bit order.
This decreases the value of the primitive being shifted.

**THAT IS NOT HOW `BitSlice` WORKS!**

`BitSlice` defines its layout with the minimum on the left and the maximum on
the right! Thus, right-shifting moves bits towards the **maximum**.

In Big-Endian order, the effect in memory will be what you expect the `>>`
operator to do.

**In LittleEndian order, the effect will be equivalent to using `<<` on the**
**fundamentals in memory!**

# Notes

In order to preserve the effects in memory that this operator traditionally
expects, the bits that are emptied by this operation are zeroed rather than left
to their old value.

The shift amount is modulated against the array length, so it is not an error to
pass a shift amount greater than the array length.

A shift amount of zero is a no-op, and returns immediately.
**/
impl<C, T> ShrAssign<usize> for BitSlice<C, T>
where C: Cursor, T: BitStore {
	/// Shifts a slice right, in place.
	///
	/// # Parameters
	///
	/// - `&mut self`
	/// - `shamt`: The shift amount. If this is greater than the length, then
	///   the slice is zeroed immediately.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::*;
	///
	/// let mut src = [0x4Bu8, 0xA5];
	/// let bits = &mut src.bits_mut::<BigEndian>()[2 .. 14];
	/// *bits >>= 3;
	/// assert_eq!(src, [0b01_000_00_1, 0b011_101_01])
	/// ```
	fn shr_assign(&mut self, shamt: usize) {
		if shamt == 0 {
			return;
		}
		let len = self.len();
		if shamt >= len {
			self.set_all(false);
			return;
		}
		//  If the slice fully owns its memory, then a fast path is available
		//  with element-wise `memmove`.
		if self.bitptr().domain().is_spanning() {
			//  Compute the shift amount measured in elements.
			let offset = shamt >> T::INDX;
			// Compute the number of elements that will remain.
			let rem = self.bitptr().elements() - offset;

			/* Memory model: suppose we have this slice of sixteen elements,
			that is shifted five elements to the right. We have two pointers
			and two lengths to manage.
			- rem is 11 (len - offset)
			- offset is 5
			- from is &[0 .. 11]
			- to is &[5 .. 16]
			  v-------before------v
			[ 0 1 2 3 4 5 6 7 8 9 a b c d e f ]
			  0 0 0 0 0 ^-------after-------^
			*/
			let from: *mut T = self.as_mut_ptr();
			let to: *mut T = &mut self.as_mut_slice()[offset];
			unsafe {
				ptr::copy(from, to, rem);
				ptr::write_bytes(from, 0, offset);
			}
			//  Any remaining shift amount only needs to shift the `after` block
			//  above.
			self[offset << T::INDX ..] >>= shamt & T::INDX as usize;
			return;
		}
		//  Otherwise, crawl.
		for (from, to) in (shamt .. len).enumerate().rev() {
			unsafe { self.copy_unchecked(from, to); }
		}
		self[.. shamt].set_all(false);
	}
}

/** Write reference to a single bit.

Rust requires that `DerefMut` produce the plain address of a value which can be
written with a `memcpy`, so, there is no way to make plain write assignments
work nicely in Rust. This reference structure is the second best option.

It contains a write reference to a single-bit slice, and a local cache `bool`.
This structure `Deref`s to the local cache, and commits the cache to the slice
on drop. This allows writing to the guard with `=` assignment.
**/
#[derive(Debug)]
pub struct BitGuard<'a, C, T>
where C: Cursor, T: 'a + BitStore {
	slot: &'a mut BitSlice<C, T>,
	bit: bool,
}

/// Read from the local cache.
impl<'a, C, T> Deref for BitGuard<'a, C, T>
where C: Cursor, T: 'a + BitStore {
	type Target = bool;

	fn deref(&self) -> &Self::Target {
		&self.bit
	}
}

/// Write to the local cache.
impl<'a, C, T> DerefMut for BitGuard<'a, C, T>
where C: Cursor, T: 'a + BitStore {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.bit
	}
}

/// Commit the local cache to the backing slice.
impl<'a, C, T> Drop for BitGuard<'a, C, T>
where C: Cursor, T: 'a + BitStore {
	fn drop(&mut self) {
		self.slot.set(0, self.bit);
	}
}

/// This type is a mutable reference with extra steps, so, it should be moveable
/// but not shareable.
#[cfg(feature = "atomic")]
unsafe impl<'a, C, T> Send for BitGuard<'a, C, T>
where C: Cursor, T: 'a + BitStore {}

mod api;
pub(crate) mod iter;
mod traits;

//  Match the `core::slice` API module topology.

pub use self::api::*;
pub use self::iter::*;

#[cfg(test)]
mod tests;
