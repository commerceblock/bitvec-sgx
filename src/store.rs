/*! Bit management

The `BitStore` trait defines constants and associated functions suitable for
managing the bit patterns of a fundamental, and is the constraint for the
storage type of the data structures of the rest of the crate.

The other types in this module provide stronger rules about how indices map to
concrete bits in fundamental elements. They are implementation details, and are
not exported in the prelude.
!*/

use crate::{
	cursor::Cursor,
	indices::BitIdx,
};

use core::{
	cmp::Eq,
	fmt::{
		Binary,
		Debug,
		Display,
		LowerHex,
		UpperHex,
	},
	mem::size_of,
	sync::atomic::Ordering::Relaxed,
	ops::{
		BitAnd,
		BitAndAssign,
		BitOr,
		BitOrAssign,
		Not,
		Shl,
		ShlAssign,
		Shr,
		ShrAssign,
	},
};

use radium::RadiumBits;

#[cfg(feature = "atomic")]
use core::sync::atomic;

#[cfg(not(feature = "atomic"))]
use core::cell::Cell;

/** Generalizes over the fundamental types for use in `bitvec` data structures.

This trait must only be implemented on unsigned integer primitives with full
alignment. It cannot be implemented on `u128` on any architecture, or on `u64`
on 32-bit systems.

The `Sealed` supertrait ensures that this can only be implemented locally, and
will never be implemented by downstream crates on new types.
**/
pub trait BitStore:
	//  Forbid external implementation
	Sealed
	+ Binary
	//  Element-wise binary manipulation
	+ BitAnd<Self, Output=Self>
	+ BitAndAssign<Self>
	+ BitOr<Self, Output=Self>
	+ BitOrAssign<Self>
	//  Permit indexing into a generic array
	+ Copy
	+ Debug
	+ Default
	+ Display
	//  Permit testing a value against 1 in `get()`.
	+ Eq
	//  Rust treats numeric literals in code as vaguely typed and does not make
	//  them concrete until long after trait expansion, so this enables building
	//  a concrete Self value from a numeric literal.
	+ From<u8>
	//  Permit extending into a `u64`.
	+ Into<u64>
	+ LowerHex
	+ Not<Output=Self>
	+ Send
	+ Shl<u8, Output=Self>
	+ ShlAssign<u8>
	+ Shr<u8, Output=Self>
	+ ShrAssign<u8>
	//  Allow direct access to a concrete implementor type.
	+ Sized
	+ Sync
	+ UpperHex
{
	/// The width, in bits, of this type.
	const BITS: u8 = size_of::<Self>() as u8 * 8;

	/// The number of bits required to index a bit inside the type. This is
	/// always log<sub>2</sub> of the type’s bit width.
	const INDX: u8 = Self::BITS.trailing_zeros() as u8;

	/// The bitmask to turn an arbitrary number into a bit index. Bit indices
	/// are always stored in the lowest bits of an index value.
	const MASK: u8 = Self::BITS - 1;

	/// Name of the implementing type. This is only necessary until the compiler
	/// stabilizes `type_name()`.
	const TYPENAME: &'static str;

	/// Shared-mutability wrapper type used to safely mutate aliased data.
	///
	/// Within `&/mut BitSlice` contexts, the `Nucleus` type **must** be used to
	/// ensure correctly-synchronized access to memory elements that may have
	/// aliased mutable access. When a codepath knows that it has full ownership
	/// of a memory element of `Self`, and no other codepath may observe, much
	/// less modify, it, then that codepath may skip the `Nucleus` type and use
	/// plain accessors.
	type Nucleus: BitAccess<Self>;

	/// Sets a specific bit in an element to a given value.
	///
	/// # Safety
	///
	/// This method cannot be called from within a `&mut BitSlice` context; it
	/// may only be called by construction of an `&mut Self` reference from a
	/// `Self` element directly.
	///
	/// # Parameters
	///
	/// - `&mut self`
	/// - `place`: A bit index in the element, from `0` to `Self::MASK`. The bit
	///   under this index will be set according to `value`.
	/// - `value`: A Boolean value, which sets the bit on `true` and clears it
	///   on `false`.
	///
	/// # Type Parameters
	///
	/// - `C`: A `Cursor` implementation to translate the index into a position.
	#[inline(always)]
	fn set<C>(&mut self, place: BitIdx<Self>, value: bool)
	where C: Cursor {
		let mask = *C::mask(place);
		if value {
			*self |= mask;
		}
		else {
			*self &= !mask;
		}
	}

	/// Gets a specific bit in an element.
	///
	/// # Safety
	///
	/// This method cannot be called from within a `&BitSlice` context; it may
	/// only be called by construction of an `&Self` reference from a `Self`
	/// element directly.
	///
	/// # Parameters
	///
	/// - `place`: A bit index in the element, from `0` to `Self::MASK`. The bit
	///   under this index will be retrieved as a `bool`.
	///
	/// # Returns
	///
	/// The value of the bit under `place`, as a `bool`.
	///
	/// # Type Parameters
	///
	/// - `C`: A `Cursor` implementation to translate the index into a position.
	fn get<C>(&self, place: BitIdx<Self>) -> bool
	where C: Cursor {
		*self & *C::mask(place) != Self::from(0)
	}

	/// Counts how many bits in `self` are set to `1`.
	///
	/// This zero-extends `self` to `u64`, and uses the [`u64::count_ones`]
	/// inherent method.
	///
	/// # Parameters
	///
	/// - `&self`
	///
	/// # Returns
	///
	/// The number of bits in `self` set to `1`. This is a `usize` instead of a
	/// `u32` in order to ease arithmetic throughout the crate.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::BitStore;
	/// assert_eq!(BitStore::count_ones(&0u8), 0);
	/// assert_eq!(BitStore::count_ones(&128u8), 1);
	/// assert_eq!(BitStore::count_ones(&192u8), 2);
	/// assert_eq!(BitStore::count_ones(&224u8), 3);
	/// assert_eq!(BitStore::count_ones(&240u8), 4);
	/// assert_eq!(BitStore::count_ones(&248u8), 5);
	/// assert_eq!(BitStore::count_ones(&252u8), 6);
	/// assert_eq!(BitStore::count_ones(&254u8), 7);
	/// assert_eq!(BitStore::count_ones(&255u8), 8);
	/// ```
	///
	/// [`u64::count_ones`]: https://doc.rust-lang.org/stable/std/primitive.u64.html#method.count_ones
	#[inline(always)]
	fn count_ones(&self) -> usize {
		u64::count_ones((*self).into()) as usize
	}

	/// Counts how many bits in `self` are set to `0`.
	///
	/// This inverts `self`, so all `0` bits are `1` and all `1` bits are `0`,
	/// then zero-extends `self` to `u64` and uses the [`u64::count_ones`]
	/// inherent method.
	///
	/// # Parameters
	///
	/// - `&self`
	///
	/// # Returns
	///
	/// The number of bits in `self` set to `0`. This is a `usize` instead of a
	/// `u32` in order to ease arithmetic throughout the crate.
	///
	/// # Examples
	///
	/// ```rust
	/// use bitvec::prelude::BitStore;
	/// assert_eq!(BitStore::count_zeros(&0u8), 8);
	/// assert_eq!(BitStore::count_zeros(&1u8), 7);
	/// assert_eq!(BitStore::count_zeros(&3u8), 6);
	/// assert_eq!(BitStore::count_zeros(&7u8), 5);
	/// assert_eq!(BitStore::count_zeros(&15u8), 4);
	/// assert_eq!(BitStore::count_zeros(&31u8), 3);
	/// assert_eq!(BitStore::count_zeros(&63u8), 2);
	/// assert_eq!(BitStore::count_zeros(&127u8), 1);
	/// assert_eq!(BitStore::count_zeros(&255u8), 0);
	/// ```
	///
	/// [`u64::count_ones`]: https://doc.rust-lang.org/stable/std/primitive.u64.html#method.count_ones
	#[inline(always)]
	fn count_zeros(&self) -> usize {
		//  invert (0 becomes 1, 1 becomes 0), zero-extend, count ones
		u64::count_ones((!*self).into()) as usize
	}

	/// Extends a single bit to fill the entire element.
	///
	/// # Parameters
	///
	/// - `bit`: The bit to extend.
	///
	/// # Returns
	///
	/// An element with all bits set to the input.
	#[inline]
	fn bits(bit: bool) -> Self {
		if bit {
			!Self::from(0)
		}
		else {
			Self::from(0)
		}
	}
}

/** Marker trait to seal `BitStore` against downstream implementation.

This trait is public in the module, so that other modules in the crate can use
it, but so long as it is not exported by the crate root and this module is
private, this trait effectively forbids downstream implementation of the
`BitStore` trait.
**/
#[doc(hidden)]
pub trait Sealed {}

macro_rules! store {
	( $( $t:ty , $a:ty $( ; )? );* ) => { $(
		impl Sealed for $t {}
		impl BitStore for $t {
			const TYPENAME: &'static str = stringify!($t);
			#[cfg(feature = "atomic")]
			type Nucleus = $a;
			#[cfg(not(feature = "atomic"))]
			type Nucleus = Cell<Self>;
		}
	)* };
}

store![
	u8, atomic::AtomicU8;
	u16, atomic::AtomicU16;
	u32, atomic::AtomicU32;
];

#[cfg(target_pointer_width = "64")]
store![u64, atomic::AtomicU64];

/// Type alias to the CPU word element, `u32`.
#[cfg(target_pointer_width = "32")]
pub type Word = u32;

/// Type alias to the CPU word element, `u64`.
#[cfg(target_pointer_width = "64")]
pub type Word = u64;

/** Single-bit interface for shared/mutable memory access.

`&/mut BitSlice` contexts must use the `BitStore::Nucleus` type for all
reference production, and must route through this trait in order to access the
underlying memory. This trait extends the `RadiumBits` element-wise shared
mutable access with single-bit operations suited for the behavior of the overall
crate.
**/
pub trait BitAccess<T>: RadiumBits<T>
where T: BitStore {
	/// Sets a specific bit in an element low.
	///
	/// `BitAccess::set` calls this when its `value` is `false`; it
	/// unconditionally writes a `0` bit into the electrical position that
	/// `place` controls according to the `Cursor` parameter `C`.
	///
	/// # Type Parameters
	///
	/// - `C`: A `Cursor` implementation which translates `place` into a usable
	///   bit-mask.
	///
	/// # Parameters
	///
	/// - `&self`
	/// - `place`: The semantic bit index in the `self` element.
	#[inline(always)]
	fn clear_bit<C>(&self, place: BitIdx<T>)
	where C: Cursor {
		self.fetch_and(!*C::mask(place), Relaxed);
	}

	/// Sets a specific bit in an element high.
	///
	/// `BitAccess::set` calls this when its `value` is `true`; it
	/// unconditionally writes a `1` bit into the electrical position that
	/// `place` controls according to the `Cursor` parameter `C`.
	///
	/// # Type Parameters
	///
	/// - `C`: A `Cursor` implementation which translates `place` into a usable
	///   bit-mask.
	///
	/// # Parameters
	///
	/// - `&self`
	/// - `place`: The semantic bit index in the `self` element.
	#[inline(always)]
	fn set_bit<C>(&self, place: BitIdx<T>)
	where C: Cursor {
		self.fetch_or(*C::mask(place), Relaxed);
	}

	/// Inverts a specific bit in an element.
	///
	/// This is the driver of `BitStore::invert_bit`, and has the same API and
	/// documented behavior.
	#[inline(always)]
	fn invert_bit<C>(&self, place: BitIdx<T>)
	where C: Cursor {
		self.fetch_xor(*C::mask(place), Relaxed);
	}

	/// Gets a specific bit in an element.
	///
	/// # Parameters
	///
	/// - `&self`: A shared reference to a maybe-mutable element. This uses the
	///   trait `load` function to ensure correct reads from memory.
	/// - `place`: A bit index in the element, from `0` to `Self::MASK`. The bit
	///   under this index will be retrieved as a `bool`.
	///
	/// # Returns
	///
	/// The value of the bit under `place`, as a `bool`.
	///
	/// # Type Parameters
	///
	/// - `C`: A `Cursor` implementation to translate the index into a position.
	#[inline(always)]
	fn get<C>(&self, place: BitIdx<T>) -> bool
	where C: Cursor {
		radium::Radium::load(self, Relaxed) & *C::mask(place) != T::from(0)
	}

	/// Sets a specific bit in an element to a given value.
	///
	/// This is the driver of `BitStore::set`, and has the same API and
	/// documented behavior.
	#[inline(always)]
	fn set<C>(&self, place: BitIdx<T>, value: bool)
	where C: Cursor {
		if value {
			self.set_bit::<C>(place);
		}
		else {
			self.clear_bit::<C>(place);
		}
	}

	/// Transforms a reference of `&[T::Nucleus]` into `&mut [T]`.
	///
	/// # Safety
	///
	/// This function is undefined when the `this` slice referent has aliasing
	/// pointers. It must only ever be called when the slice referent is
	/// guaranteed to have no aliases, but mutability has been removed from the
	/// type system at an earlier point in the call stack.
	///
	/// # Parameters
	///
	/// - `this`: A slice reference to some shared-mutability reference type.
	///
	/// # Returns
	///
	/// A mutable reference to the wrapped interior type of the `this` referent.
	#[inline(always)]
	unsafe fn base_slice_mut(this: &[Self]) -> &mut [T]
	where Self: Sized {
		&mut *(this as *const [Self] as *const [T] as *mut [T])
	}

	/// Performs a synchronized load on an unsynchronized reference.
	#[inline(always)]
	fn load(&self) -> T {
		radium::Radium::load(self, Relaxed)
	}

	/// Alias for `radium::RadiumBits::fetch_and(self, mask, Relaxed)`.
	///
	/// # Parameters
	///
	/// - `&self`
	/// - `mask`: A bitmask. The zero bits in the mask will set low the
	///   corresponding bits of `self`; the one bits in the mask will leave the
	///   corresponding bits untouched.
	#[inline(always)]
	fn erase_bits(&self, mask: T) {
		self.fetch_and(mask, Relaxed);
	}

	/// Alias for `radium::RadiumBits::fetch_or(self, mask, Relaxed)`.
	///
	/// # Parameters
	///
	/// - `&self`
	/// - `mask`: A bitmask. The one bits in the mask will set high the
	///   corresponding bits of `self`; the zero bits in the mask will leave the
	///   corresponding bits untouched.
	#[inline(always)]
	fn write_bits(&self, mask: T) {
		self.fetch_or(mask, Relaxed);
	}
}

impl<T, R> BitAccess<T> for R
where T: BitStore, R: RadiumBits<T> {}
