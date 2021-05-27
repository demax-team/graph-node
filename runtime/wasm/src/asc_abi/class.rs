use anyhow::anyhow;
use ethabi;
use graph::{
    data::store,
    runtime::{AscHeap, AscIndexId, AscType, AscValue, IndexForAscTypeId},
};
use graph::{prelude::serde_json, runtime::DeterministicHostError};
use graph::{prelude::slog, runtime::AscPtr};
use graph_runtime_derive::AscType;
use std::marker::PhantomData;
use std::mem::{size_of, size_of_val};

///! Rust types that have with a direct correspondence to an Asc class,
///! with their `AscType` implementations.

/// Asc std ArrayBuffer: "a generic, fixed-length raw binary data buffer".
/// See https://github.com/AssemblyScript/assemblyscript/wiki/Memory-Layout-&-Management#arrays
pub(crate) struct ArrayBuffer {
    byte_length: u32,
    // Asc allocators always align at 8 bytes, we already have 4 bytes from
    // `byte_length_size` so with 4 more bytes we align the contents at 8
    // bytes. No Asc type has alignment greater than 8, so the
    // elements in `content` will be aligned for any element type.
    // padding: [u8; 4],
    // In Asc this slice is layed out inline with the ArrayBuffer.
    content: Box<[u8]>,
}

impl ArrayBuffer {
    fn new<T: AscType>(values: &[T]) -> Result<Self, DeterministicHostError> {
        println!("ArrayBuffer::new");
        let mut content = Vec::new();
        // this should be correct
        for value in values {
            let asc_bytes = value.to_asc_bytes()?;
            println!("value.to_asc_bytes: {:?}", asc_bytes);
            // An `AscValue` has size equal to alignment, no padding required.
            content.extend(&asc_bytes);
        }

        if content.len() > u32::max_value() as usize {
            return Err(DeterministicHostError(anyhow::anyhow!(
                "slice cannot fit in WASM memory"
            )));
        }
        Ok(ArrayBuffer {
            byte_length: content.len() as u32,
            // padding: [0; 4],
            content: content.into(),
        })
    }

    /// Read `length` elements of type `T` starting at `byte_offset`.
    ///
    /// Panics if that tries to read beyond the length of `self.content`.
    fn get<T: AscType>(
        &self,
        byte_offset: u32,
        length: u32,
    ) -> Result<Vec<T>, DeterministicHostError> {
        let length = length as usize;
        let byte_offset = byte_offset as usize;

        dbg!(length);
        dbg!(byte_offset);

        self.content[byte_offset..]
            .chunks(size_of::<T>())
            .take(length)
            .map(T::from_asc_bytes)
            .collect()

        // TODO: This code is preferred as it validates the length of the array.
        // But, some existing subgraphs were found to break when this was added.
        // This needs to be root caused
        /*
        let range = byte_offset..byte_offset + length * size_of::<T>();
        self.content
            .get(range)
            .ok_or_else(|| {
                DeterministicHostError(anyhow::anyhow!("Attempted to read past end of array"))
            })?
            .chunks_exact(size_of::<T>())
            .map(|bytes| T::from_asc_bytes(bytes))
            .collect()
            */
    }
}

impl AscIndexId for ArrayBuffer {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::ArrayBuffer);
}

impl AscType for ArrayBuffer {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        println!("ArrayBuffer::to_asc_bytes");
        let mut asc_layout: Vec<u8> = Vec::new();

        // let byte_length: [u8; 4] = self.byte_length.to_le_bytes();
        // asc_layout.extend(&byte_length);
        // asc_layout.extend(&self.padding);
        // for c in self.content.iter() {
        //     let le = c.to_le_bytes();
        //     asc_layout.extend(le);
        // }
        asc_layout.extend(self.content.iter());

        // Allocate extra capacity to next power of two, as required by asc.
        println!("self.byte_length: {}", self.byte_length);
        let header_size = 20;
        let total_size = self.byte_length as usize + header_size;
        println!("total_size: {}", total_size);
        let total_capacity = total_size.next_power_of_two();
        println!("total_capacity: {}", total_capacity);
        let extra_capacity = total_capacity - total_size;
        println!("extra_capacity: {}", extra_capacity);
        // let extra = extra_capacity - self.byte_length as usize;
        // println!("extra: {}", extra);
        asc_layout.extend(std::iter::repeat(0).take(extra_capacity));
        // asc_layout.extend(std::iter::repeat(0).take(extra));
        println!(
            "assert_eq: asc_layout.len(), total_capacity: {}, {}",
            asc_layout.len(),
            total_capacity
        );
        // assert_eq!(asc_layout.len() + header_size, total_capacity);
        // assert_eq!(asc_layout.len(), total_capacity);
        println!("asc_layout: {:?}", asc_layout);

        Ok(asc_layout)
    }

    /// The Rust representation of an Asc object as layed out in Asc memory.
    fn from_asc_bytes(asc_obj: &[u8]) -> Result<Self, DeterministicHostError> {
        println!("ArrayBuffer::from_asc_bytes");
        println!("asc_obj: {:?}", asc_obj);
        Ok(ArrayBuffer{
            byte_length: asc_obj.len() as u32,
            content: asc_obj.to_vec().into(),
        })
        // Self::new(asc_obj)
        // Skip `byte_length` and the padding.
        // let content_offset = size_of::<u32>() + 4;
        // let byte_length = asc_obj.get(..size_of::<u32>()).ok_or_else(|| {
        //     DeterministicHostError(anyhow!("Attempted to read past end of array"))
        // })?;
        // let content = asc_obj.get(content_offset..).ok_or_else(|| {
        //     DeterministicHostError(anyhow!("Attempted to read past end of array"))
        // })?;
        // Ok(ArrayBuffer {
        //     byte_length: asc_obj.len(),
        //     // padding: [0; 4],
        //     content: content.to_vec().into(),
        // })
    }

    fn content_len(&self, _asc_bytes: &[u8]) -> usize {
        self.byte_length as usize // without extra_capacity
    }
}

/// A typed, indexable view of an `ArrayBuffer` of Asc primitives. In Asc it's
/// an abstract class with subclasses for each primitive, for example
/// `Uint8Array` is `TypedArray<u8>`.
///  See https://github.com/AssemblyScript/assemblyscript/wiki/Memory-Layout-&-Management#arrays
#[repr(C)]
#[derive(AscType)]
pub(crate) struct TypedArray<T> {
    pub buffer: AscPtr<ArrayBuffer>,
    /// Byte position in `buffer` of the array start.
    data_start: u32,
    byte_length: u32,
    ty: PhantomData<T>,
}

impl<T: AscValue> TypedArray<T> {
    pub(crate) fn new<H: AscHeap>(
        content: &[T],
        heap: &mut H,
    ) -> Result<Self, DeterministicHostError> {
        println!("TypedArray::new");
        println!("full type: {}", std::any::type_name::<Self>());
        let buffer = ArrayBuffer::new(content)?;
        let byte_length = buffer.byte_length;
        let ptr = AscPtr::alloc_obj(buffer, heap)?;
        println!("byte_length vs read_len: {} vs {}", byte_length, ptr.read_len(heap)?);
        // println!("byte_length again: {}", byte_length);
        let byte_length = ptr.read_len(heap)?;
        println!("ptr: {:?}", ptr);
        Ok(TypedArray {
            buffer: ptr,
            data_start: ptr.wasm_ptr(),
            byte_length,
            ty: PhantomData,
        })
    }

    pub(crate) fn to_vec<H: AscHeap>(&self, heap: &H) -> Result<Vec<T>, DeterministicHostError> {
        println!("TypedArray::to_vec");
        println!("self.buffer.wasm_ptr(): {}", self.buffer.wasm_ptr());
        println!("self.data_start: {}", self.data_start);
        println!("self.byte_length: {}", self.byte_length);
        println!("size_of::<T>: {}", size_of::<T>());
        self.buffer
            .read_ptr(heap)?
            // .get(self.byte_length, self.byte_length / size_of::<T>() as u32)
            .get(self.data_start - self.buffer.wasm_ptr(), self.byte_length / size_of::<T>() as u32)
    }
}

pub(crate) type Uint8Array = TypedArray<u8>;

impl AscIndexId for TypedArray<i8> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::Int8Array);
}

impl AscIndexId for TypedArray<i16> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::Int16Array);
}

impl AscIndexId for TypedArray<i32> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::Int32Array);
}

impl AscIndexId for TypedArray<i64> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::Int64Array);
}

impl AscIndexId for TypedArray<u8> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::Uint8Array);
}

impl AscIndexId for TypedArray<u16> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::Uint16Array);
}

impl AscIndexId for TypedArray<u32> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::Uint32Array);
}

impl AscIndexId for TypedArray<u64> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::Uint64Array);
}

impl AscIndexId for TypedArray<f32> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::Float32Array);
}

impl AscIndexId for TypedArray<f64> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::Float64Array);
}

/// Asc std string: "Strings are encoded as UTF-16LE in AssemblyScript, and are
/// prefixed with their length (in character codes) as a 32-bit integer". See
/// https://github.com/AssemblyScript/assemblyscript/wiki/Memory-Layout-&-Management#strings
pub(crate) struct AscString {
    // The sequence of UTF-16LE code units that form the string.
    byte_length: u32,
    pub content: Box<[u16]>,
}

impl AscString {
    pub fn new(content: &[u16]) -> Result<Self, DeterministicHostError> {
        if size_of_val(content) > u32::max_value() as usize {
            return Err(DeterministicHostError(anyhow!(
                "string cannot fit in WASM memory"
            )));
        }

        Ok(AscString {
            byte_length: content.len() as u32,
            content: content.into(),
        })
    }
}

impl AscIndexId for AscString {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::String);
}

impl AscType for AscString {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        let mut content: Vec<u8> = Vec::new();

        // Write the code points, in little-endian (LE) order.
        for &code_unit in self.content.iter() {
            let low_byte = code_unit as u8;
            let high_byte = (code_unit >> 8) as u8;
            content.push(low_byte);
            content.push(high_byte);
        }

        // println!("AscString::to_asc_bytes.content: {:?}", self.content);

        let header_size = 20;
        let total_size = (self.byte_length as usize * 2) + header_size;
        println!("total_size: {}", total_size);
        let total_capacity = total_size.next_power_of_two();
        println!("total_capacity: {}", total_capacity);
        let extra_capacity = total_capacity - total_size;
        println!("extra_capacity: {}", extra_capacity);
        // let extra = extra_capacity - (self.byte_length as usize * 2);
        // println!("extra: {}", extra);
        content.extend(std::iter::repeat(0).take(extra_capacity));
        // content.extend(std::iter::repeat(0).take(extra));
        println!(
            "assert_eq: asc_layout.len(), total_capacity: {}, {}",
            content.len(),
            total_capacity
        );

        Ok(content)
    }

    /// The Rust representation of an Asc object as layed out in Asc memory.
    fn from_asc_bytes(asc_obj: &[u8]) -> Result<Self, DeterministicHostError> {
        println!("AscString::from_asc_bytes");
        println!("asc_obj: {:?}", asc_obj);
        // UTF-16 (used in assemblyscript) always uses one
        // pair of bytes per code unit.
        // https://mathiasbynens.be/notes/javascript-encoding
        // UTF-16 (16-bit Unicode Transformation Format) is an
        // extension of UCS-2 that allows representing code points
        // outside the BMP. It produces a variable-length result
        // of either one or two 16-bit code units per code point.
        // This way, it can encode code points in the range from 0
        // to 0x10FFFF.

        // ~lib/rt/stub.ts
        // [
        //     126, 0,
        //     108, 0,
        //     105, 0,
        //     98, 0,
        //     47, 0,
        //     114, 0,
        //     116, 0,
        //     47, 0,
        //     115, 0,
        //     116, 0,
        //     117, 0,
        //     98, 0,
        //     46, 0,
        //     116, 0,
        //     115, 0
        // ]
        let mut content = Vec::new();
        for pair in asc_obj.chunks(2) {
            let code_point_bytes = [
                pair[0],
                *pair.get(1).ok_or_else(|| {
                    DeterministicHostError(anyhow!(
                        "Attempted to read past end of string content bytes chunk"
                    ))
                })?,
            ];
            let code_point = u16::from_le_bytes(code_point_bytes);
            content.push(code_point);
        }

        println!("after content: {:?}", content);
        AscString::new(&content)
    }

    fn content_len(&self, _asc_bytes: &[u8]) -> usize {
        self.byte_length as usize * 2 // without extra_capacity
    }
}

/// Growable array backed by an `ArrayBuffer`.
/// See https://github.com/AssemblyScript/assemblyscript/wiki/Memory-Layout-&-Management#arrays
#[repr(C)]
#[derive(AscType)]
pub(crate) struct Array<T> {
    buffer: AscPtr<ArrayBuffer>,
    buffer_data_start: u32,
    buffer_data_length: u32,
    length: i32,
    ty: PhantomData<T>,
}

impl<T: AscValue> Array<T> {
    pub fn new<H: AscHeap>(content: &[T], heap: &mut H) -> Result<Self, DeterministicHostError> {
        println!("Array::new");
        let buffer = AscPtr::alloc_obj(ArrayBuffer::new(content)?, heap)?;
        println!("buffer_data_start: {}", buffer.wasm_ptr());
        let buffer_data_length = buffer.read_len(heap)?;
        println!("buffer_data_length: {}", buffer_data_length);

        Ok(Array {
            buffer,
            buffer_data_start: buffer.wasm_ptr(),
            buffer_data_length,
            length: content.len() as i32,
            ty: PhantomData,
        })
    }

    pub(crate) fn to_vec<H: AscHeap>(&self, heap: &H) -> Result<Vec<T>, DeterministicHostError> {
        // self.buffer.read_ptr(heap)?.get(0, self.buffer_data_length)
        // self.buffer.read_ptr(heap)?.get(self.buffer_data_start - self.buffer.wasm_ptr(), self.length as u32)
        self.buffer.read_ptr(heap)?.get(self.buffer_data_start - self.buffer.wasm_ptr(), self.buffer_data_length)
    }
}

impl AscIndexId for Array<bool> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::ArrayBool);
}

impl AscIndexId for Array<Uint8Array> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::ArrayUint8Array);
}

impl AscIndexId for Array<AscPtr<AscEnum<EthereumValueKind>>> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> =
        Some(IndexForAscTypeId::ArrayEthereumValue);
}

impl AscIndexId for Array<AscPtr<AscEnum<StoreValueKind>>> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::ArrayStoreValue);
}

impl AscIndexId for Array<AscPtr<AscEnum<JsonValueKind>>> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::ArrayJsonValue);
}

impl AscIndexId for Array<AscPtr<AscString>> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::ArrayString);
}

impl AscIndexId for Array<AscPtr<AscLogParam>> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::ArrayEventParam);
}

impl AscIndexId for Array<AscPtr<AscTypedMapEntry<AscString, AscEnum<JsonValueKind>>>> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> =
        Some(IndexForAscTypeId::ArrayTypedMapEntryStringJsonValue);
}

impl AscIndexId for Array<AscPtr<AscTypedMapEntry<AscString, AscEnum<StoreValueKind>>>> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> =
        Some(IndexForAscTypeId::ArrayTypedMapEntryStringStoreValue);
}

impl AscIndexId for Array<u8> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::ArrayU8);
}

impl AscIndexId for Array<u16> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::ArrayU16);
}

impl AscIndexId for Array<u32> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::ArrayU32);
}

impl AscIndexId for Array<u64> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::ArrayU64);
}

impl AscIndexId for Array<i8> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::ArrayI8);
}

impl AscIndexId for Array<i16> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::ArrayI16);
}

impl AscIndexId for Array<i32> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::ArrayI32);
}

impl AscIndexId for Array<i64> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::ArrayI64);
}

impl AscIndexId for Array<f32> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::ArrayF32);
}

impl AscIndexId for Array<f64> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::ArrayF64);
}

impl AscIndexId for Array<AscPtr<AscBigDecimal>> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::ArrayBigDecimal);
}

/// Represents any `AscValue` since they all fit in 64 bits.
#[repr(C)]
#[derive(Copy, Clone, Default)]
pub(crate) struct EnumPayload(pub u64);

impl AscType for EnumPayload {
    fn to_asc_bytes(&self) -> Result<Vec<u8>, DeterministicHostError> {
        self.0.to_asc_bytes()
    }

    fn from_asc_bytes(asc_obj: &[u8]) -> Result<Self, DeterministicHostError> {
        Ok(EnumPayload(u64::from_asc_bytes(asc_obj)?))
    }
}

impl From<EnumPayload> for i32 {
    fn from(payload: EnumPayload) -> i32 {
        payload.0 as i32
    }
}

impl From<EnumPayload> for f64 {
    fn from(payload: EnumPayload) -> f64 {
        f64::from_bits(payload.0)
    }
}

impl From<EnumPayload> for bool {
    fn from(payload: EnumPayload) -> bool {
        payload.0 != 0
    }
}

impl From<i32> for EnumPayload {
    fn from(x: i32) -> EnumPayload {
        EnumPayload(x as u64)
    }
}

impl From<f64> for EnumPayload {
    fn from(x: f64) -> EnumPayload {
        EnumPayload(x.to_bits())
    }
}

impl From<bool> for EnumPayload {
    fn from(b: bool) -> EnumPayload {
        EnumPayload(if b { 1 } else { 0 })
    }
}

impl From<i64> for EnumPayload {
    fn from(x: i64) -> EnumPayload {
        EnumPayload(x as u64)
    }
}

impl<C> From<EnumPayload> for AscPtr<C> {
    fn from(payload: EnumPayload) -> Self {
        AscPtr::new(payload.0 as u32)
    }
}

impl<C> From<AscPtr<C>> for EnumPayload {
    fn from(x: AscPtr<C>) -> EnumPayload {
        EnumPayload(x.wasm_ptr() as u64)
    }
}

/// In Asc, we represent a Rust enum as a discriminant `kind: D`, which is an
/// Asc enum so in Rust it's a `#[repr(u32)]` enum, plus an arbitrary `AscValue`
/// payload.
#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEnum<D: AscValue> {
    pub kind: D,
    pub _padding: u32, // Make padding explicit.
    pub payload: EnumPayload,
}

impl AscIndexId for AscEnum<EthereumValueKind> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::EthereumValue);
}

impl AscIndexId for AscEnum<StoreValueKind> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::StoreValue);
}

impl AscIndexId for AscEnum<JsonValueKind> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::JsonValue);
}

pub(crate) type AscEnumArray<D> = AscPtr<Array<AscPtr<AscEnum<D>>>>;

#[repr(u32)]
#[derive(AscType, Copy, Clone)]
pub(crate) enum EthereumValueKind {
    Address,
    FixedBytes,
    Bytes,
    Int,
    Uint,
    Bool,
    String,
    FixedArray,
    Array,
    Tuple,
}

impl EthereumValueKind {
    pub(crate) fn get_kind(token: &ethabi::Token) -> Self {
        match token {
            ethabi::Token::Address(_) => EthereumValueKind::Address,
            ethabi::Token::FixedBytes(_) => EthereumValueKind::FixedBytes,
            ethabi::Token::Bytes(_) => EthereumValueKind::Bytes,
            ethabi::Token::Int(_) => EthereumValueKind::Int,
            ethabi::Token::Uint(_) => EthereumValueKind::Uint,
            ethabi::Token::Bool(_) => EthereumValueKind::Bool,
            ethabi::Token::String(_) => EthereumValueKind::String,
            ethabi::Token::FixedArray(_) => EthereumValueKind::FixedArray,
            ethabi::Token::Array(_) => EthereumValueKind::Array,
            ethabi::Token::Tuple(_) => EthereumValueKind::Tuple,
        }
    }
}

impl Default for EthereumValueKind {
    fn default() -> Self {
        EthereumValueKind::Address
    }
}

impl AscValue for EthereumValueKind {}

#[repr(u32)]
#[derive(AscType, Copy, Clone)]
pub enum StoreValueKind {
    String,
    Int,
    BigDecimal,
    Bool,
    Array,
    Null,
    Bytes,
    BigInt,
}

impl StoreValueKind {
    pub(crate) fn get_kind(value: &store::Value) -> StoreValueKind {
        use self::store::Value;

        match value {
            Value::String(_) => StoreValueKind::String,
            Value::Int(_) => StoreValueKind::Int,
            Value::BigDecimal(_) => StoreValueKind::BigDecimal,
            Value::Bool(_) => StoreValueKind::Bool,
            Value::List(_) => StoreValueKind::Array,
            Value::Null => StoreValueKind::Null,
            Value::Bytes(_) => StoreValueKind::Bytes,
            Value::BigInt(_) => StoreValueKind::BigInt,
        }
    }
}

impl Default for StoreValueKind {
    fn default() -> Self {
        StoreValueKind::Null
    }
}

impl AscValue for StoreValueKind {}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscLogParam {
    pub name: AscPtr<AscString>,
    pub value: AscPtr<AscEnum<EthereumValueKind>>,
}

impl AscIndexId for AscLogParam {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::EventParam);
}

pub(crate) type Bytes = Uint8Array;

/// Big ints are represented using signed number representation. Note: This differs
/// from how U256 and U128 are represented (they use two's complement). So whenever
/// we convert between them, we need to make sure we handle signed and unsigned
/// cases correctly.
pub(crate) type AscBigInt = Uint8Array;

pub(crate) type AscAddress = Uint8Array;
pub(crate) type AscH160 = Uint8Array;
pub(crate) type AscH256 = Uint8Array;

pub(crate) type AscLogParamArray = Array<AscPtr<AscLogParam>>;

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEthereumBlock {
    pub hash: AscPtr<AscH256>,
    pub parent_hash: AscPtr<AscH256>,
    pub uncles_hash: AscPtr<AscH256>,
    pub author: AscPtr<AscH160>,
    pub state_root: AscPtr<AscH256>,
    pub transactions_root: AscPtr<AscH256>,
    pub receipts_root: AscPtr<AscH256>,
    pub number: AscPtr<AscBigInt>,
    pub gas_used: AscPtr<AscBigInt>,
    pub gas_limit: AscPtr<AscBigInt>,
    pub timestamp: AscPtr<AscBigInt>,
    pub difficulty: AscPtr<AscBigInt>,
    pub total_difficulty: AscPtr<AscBigInt>,
    pub size: AscPtr<AscBigInt>,
}

impl AscIndexId for AscEthereumBlock {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::EthereumBlock);
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEthereumTransaction {
    pub hash: AscPtr<AscH256>,
    pub index: AscPtr<AscBigInt>,
    pub from: AscPtr<AscH160>,
    pub to: AscPtr<AscH160>,
    pub value: AscPtr<AscBigInt>,
    pub gas_used: AscPtr<AscBigInt>,
    pub gas_price: AscPtr<AscBigInt>,
}

impl AscIndexId for AscEthereumTransaction {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> =
        Some(IndexForAscTypeId::EthereumTransaction);
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEthereumTransaction_0_0_2 {
    pub hash: AscPtr<AscH256>,
    pub index: AscPtr<AscBigInt>,
    pub from: AscPtr<AscH160>,
    pub to: AscPtr<AscH160>,
    pub value: AscPtr<AscBigInt>,
    pub gas_used: AscPtr<AscBigInt>,
    pub gas_price: AscPtr<AscBigInt>,
    pub input: AscPtr<Bytes>,
}

impl AscIndexId for AscEthereumTransaction_0_0_2 {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> =
        Some(IndexForAscTypeId::EthereumTransaction);
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEthereumEvent<T>
where
    T: AscType,
{
    pub address: AscPtr<AscAddress>,
    pub log_index: AscPtr<AscBigInt>,
    pub transaction_log_index: AscPtr<AscBigInt>,
    pub log_type: AscPtr<AscString>,
    pub block: AscPtr<AscEthereumBlock>,
    pub transaction: AscPtr<T>,
    pub params: AscPtr<AscLogParamArray>,
}

impl AscIndexId for AscEthereumEvent<AscEthereumTransaction> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::EthereumEvent);
}

impl AscIndexId for AscEthereumEvent<AscEthereumTransaction_0_0_2> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::EthereumEvent);
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEthereumCall {
    pub address: AscPtr<AscAddress>,
    pub block: AscPtr<AscEthereumBlock>,
    pub transaction: AscPtr<AscEthereumTransaction>,
    pub inputs: AscPtr<AscLogParamArray>,
    pub outputs: AscPtr<AscLogParamArray>,
}

impl AscIndexId for AscEthereumCall {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::EthereumCall);
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscEthereumCall_0_0_3 {
    pub to: AscPtr<AscAddress>,
    pub from: AscPtr<AscAddress>,
    pub block: AscPtr<AscEthereumBlock>,
    pub transaction: AscPtr<AscEthereumTransaction>,
    pub inputs: AscPtr<AscLogParamArray>,
    pub outputs: AscPtr<AscLogParamArray>,
}

impl AscIndexId for AscEthereumCall_0_0_3 {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::EthereumCall);
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscTypedMapEntry<K, V> {
    pub key: AscPtr<K>,
    pub value: AscPtr<V>,
}

impl AscIndexId for AscTypedMapEntry<AscString, AscEnum<StoreValueKind>> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> =
        Some(IndexForAscTypeId::TypedMapEntryStringStoreValue);
}

impl AscIndexId for AscTypedMapEntry<AscString, AscEnum<JsonValueKind>> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> =
        Some(IndexForAscTypeId::TypedMapEntryStringJsonValue);
}

pub(crate) type AscTypedMapEntryArray<K, V> = Array<AscPtr<AscTypedMapEntry<K, V>>>;

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscTypedMap<K, V> {
    pub entries: AscPtr<AscTypedMapEntryArray<K, V>>,
}

impl AscIndexId for AscTypedMap<AscString, AscEnum<StoreValueKind>> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> =
        Some(IndexForAscTypeId::TypedMapStringStoreValue);
}

impl AscIndexId for AscTypedMap<AscString, AscEnum<JsonValueKind>> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> =
        Some(IndexForAscTypeId::TypedMapStringJsonValue);
}

impl AscIndexId for AscTypedMap<AscString, AscTypedMap<AscString, AscEnum<JsonValueKind>>> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> =
        Some(IndexForAscTypeId::TypedMapStringTypedMapStringJsonValue);
}

pub(crate) type AscEntity = AscTypedMap<AscString, AscEnum<StoreValueKind>>;
pub(crate) type AscJson = AscTypedMap<AscString, AscEnum<JsonValueKind>>;

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscUnresolvedContractCall {
    pub contract_name: AscPtr<AscString>,
    pub contract_address: AscPtr<AscAddress>,
    pub function_name: AscPtr<AscString>,
    pub function_args: AscPtr<Array<AscPtr<AscEnum<EthereumValueKind>>>>,
}

impl AscIndexId for AscUnresolvedContractCall {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::SmartContractCall);
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscUnresolvedContractCall_0_0_4 {
    pub contract_name: AscPtr<AscString>,
    pub contract_address: AscPtr<AscAddress>,
    pub function_name: AscPtr<AscString>,
    pub function_signature: AscPtr<AscString>,
    pub function_args: AscPtr<Array<AscPtr<AscEnum<EthereumValueKind>>>>,
}

impl AscIndexId for AscUnresolvedContractCall_0_0_4 {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::SmartContractCall);
}

#[repr(u32)]
#[derive(AscType, Copy, Clone)]
pub(crate) enum JsonValueKind {
    Null,
    Bool,
    Number,
    String,
    Array,
    Object,
}

impl Default for JsonValueKind {
    fn default() -> Self {
        JsonValueKind::Null
    }
}

impl AscValue for JsonValueKind {}

impl JsonValueKind {
    pub(crate) fn get_kind(token: &serde_json::Value) -> Self {
        use serde_json::Value;

        match token {
            Value::Null => JsonValueKind::Null,
            Value::Bool(_) => JsonValueKind::Bool,
            Value::Number(_) => JsonValueKind::Number,
            Value::String(_) => JsonValueKind::String,
            Value::Array(_) => JsonValueKind::Array,
            Value::Object(_) => JsonValueKind::Object,
        }
    }
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscBigDecimal {
    pub digits: AscPtr<AscBigInt>,

    // Decimal exponent. This is the opposite of `scale` in rust BigDecimal.
    pub exp: AscPtr<AscBigInt>,
}

impl AscIndexId for AscBigDecimal {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::BigDecimal);
}

#[repr(u32)]
pub(crate) enum LogLevel {
    Critical,
    Error,
    Warning,
    Info,
    Debug,
}

impl From<LogLevel> for slog::Level {
    fn from(level: LogLevel) -> slog::Level {
        match level {
            LogLevel::Critical => slog::Level::Critical,
            LogLevel::Error => slog::Level::Error,
            LogLevel::Warning => slog::Level::Warning,
            LogLevel::Info => slog::Level::Info,
            LogLevel::Debug => slog::Level::Debug,
        }
    }
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscResult<V, E> {
    pub value: AscPtr<AscWrapped<V>>,
    pub error: AscPtr<AscWrapped<E>>,
}

impl AscIndexId for AscResult<AscJson, bool> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> =
        Some(IndexForAscTypeId::ResultTypedMapStringJsonValueBool);
}

impl AscIndexId for AscResult<AscEnum<JsonValueKind>, bool> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> =
        Some(IndexForAscTypeId::ResultJsonValueBool);
}

#[repr(C)]
#[derive(AscType)]
pub(crate) struct AscWrapped<V> {
    pub inner: AscPtr<V>,
}

impl AscIndexId for AscWrapped<AscJson> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> =
        Some(IndexForAscTypeId::WrappedTypedMapStringJsonValue);
}

impl AscIndexId for AscWrapped<bool> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::WrappedBool);
}

impl AscIndexId for AscWrapped<AscEnum<JsonValueKind>> {
    const INDEX_ASC_TYPE_ID: Option<IndexForAscTypeId> = Some(IndexForAscTypeId::WrappedJsonValue);
}

impl<V> Copy for AscWrapped<V> {}

impl<V> Clone for AscWrapped<V> {
    fn clone(&self) -> Self {
        Self { inner: self.inner }
    }
}
