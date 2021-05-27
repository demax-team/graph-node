use semver::Version;

use super::{AscIndexId, AscPtr, AscType, DeterministicHostError, IndexForAscTypeId};
/// A type that can read and write to the Asc heap. Call `asc_new` and `asc_get`
/// for reading and writing Rust structs from and to Asc.
///
/// The implementor must provide the direct Asc interface with `raw_new` and `get`.
pub trait AscHeap: Sized {
    /// Allocate new space and write `bytes`, return the allocated address.
    fn raw_new(&mut self, bytes: &[u8]) -> Result<u32, DeterministicHostError>;

    fn get(&self, offset: u32, size: u32) -> Result<Vec<u8>, DeterministicHostError>;

    /// Instantiate `rust_obj` as an Asc object of class `C`.
    /// Returns a pointer to the Asc heap.
    ///
    /// This operation is expensive as it requires a call to `raw_new` for every
    /// nested object.
    fn asc_new<C, T: ?Sized>(&mut self, rust_obj: &T) -> Result<AscPtr<C>, DeterministicHostError>
    where
        C: AscType + AscIndexId,
        T: ToAscObj<C>,
    {
        let obj = rust_obj.to_asc_obj(self)?;
        AscPtr::alloc_obj(obj, self)
    }

    ///  Read the rust representation of an Asc object of class `C`.
    ///
    ///  This operation is expensive as it requires a call to `get` for every
    ///  nested object.
    fn asc_get<T, C>(&self, asc_ptr: AscPtr<C>) -> Result<T, DeterministicHostError>
    where
        C: AscType + AscIndexId,
        T: FromAscObj<C>,
    {
        T::from_asc_obj(asc_ptr.read_ptr(self)?, self)
    }

    fn try_asc_get<T, C>(&self, asc_ptr: AscPtr<C>) -> Result<T, DeterministicHostError>
    where
        C: AscType + AscIndexId,
        T: TryFromAscObj<C>,
    {
        let a = asc_ptr.read_ptr(self)?;
        T::try_from_asc_obj(a, self)
    }

    fn api_version(&self) -> Version;

    fn asc_type_id(&mut self, type_id_index: IndexForAscTypeId) -> u32;
}

/// Type that can be converted to an Asc object of class `C`.
pub trait ToAscObj<C: AscType> {
    fn to_asc_obj<H: AscHeap>(&self, heap: &mut H) -> Result<C, DeterministicHostError>;
}

impl AscIndexId for bool {}

impl ToAscObj<bool> for bool {
    fn to_asc_obj<H: AscHeap>(&self, _heap: &mut H) -> Result<bool, DeterministicHostError> {
        Ok(*self)
    }
}

impl<C: AscType, T: ToAscObj<C>> ToAscObj<C> for &T {
    fn to_asc_obj<H: AscHeap>(&self, heap: &mut H) -> Result<C, DeterministicHostError> {
        (*self).to_asc_obj(heap)
    }
}

/// Type that can be converted from an Asc object of class `C`.
pub trait FromAscObj<C: AscType> {
    fn from_asc_obj<H: AscHeap>(obj: C, heap: &H) -> Result<Self, DeterministicHostError>
    where
        Self: Sized;
}

pub trait TryFromAscObj<C: AscType>: Sized {
    fn try_from_asc_obj<H: AscHeap>(obj: C, heap: &H) -> Result<Self, DeterministicHostError>;
}
