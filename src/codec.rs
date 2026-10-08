//! Optional fixed-offset records with shared strings and bytes.
//!
//! Store encoder metadata in index metadata or a separate immutable file. Field
//! positions belong to the caller's schema; no field names occur in each record.
//! Replace records and metadata together. Decoding borrows the caller's bytes.

use crate::{
    Error, Limits,
    values::{StringPool, ValuePool, ValuePoolBuilder},
};

const MAGIC: &[u8; 8] = b"NETCODEC";
const HEADER: usize = 12;

/// Scalar or shared-value encoding for one field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    /// A boolean encoded as zero or one.
    Bool = 0,
    /// An unsigned eight-bit integer.
    U8 = 1,
    /// An unsigned sixteen-bit integer.
    U16 = 2,
    /// An unsigned thirty-two-bit integer.
    U32 = 3,
    /// An unsigned sixty-four-bit integer.
    U64 = 4,
    /// A signed sixty-four-bit integer.
    I64 = 5,
    /// An IEEE 754 double, preserving its bits.
    F64 = 6,
    /// A four-byte reference to a shared UTF-8 string.
    String = 7,
    /// A four-byte reference to shared opaque bytes.
    Bytes = 8,
}

impl Kind {
    fn width(self) -> usize {
        match self {
            Self::Bool | Self::U8 => 1,
            Self::U16 => 2,
            Self::U32 | Self::String | Self::Bytes => 4,
            Self::U64 | Self::I64 | Self::F64 => 8,
        }
    }

    fn decode(tag: u8) -> Result<Self, Error> {
        match tag {
            0 => Ok(Self::Bool),
            1 => Ok(Self::U8),
            2 => Ok(Self::U16),
            3 => Ok(Self::U32),
            4 => Ok(Self::U64),
            5 => Ok(Self::I64),
            6 => Ok(Self::F64),
            7 => Ok(Self::String),
            8 => Ok(Self::Bytes),
            _ => Err(Error::Invalid("codec field kind")),
        }
    }
}

/// Whether a field occupies a slot and may be missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// Every record supplies a value of this kind.
    Required(Kind),
    /// A presence bit distinguishes missing from zero or empty.
    Optional(Kind),
    /// The field is missing in every record and occupies no record bytes.
    Absent(Kind),
}

impl Field {
    fn kind(self) -> Kind {
        match self {
            Self::Required(kind) | Self::Optional(kind) | Self::Absent(kind) => kind,
        }
    }

    fn mode(self) -> u8 {
        match self {
            Self::Required(_) => 0,
            Self::Optional(_) => 1,
            Self::Absent(_) => 2,
        }
    }
}

/// A field supplied to the encoder or borrowed from a decoded record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value<'a> {
    /// A missing optional or globally absent field.
    Null,
    /// A boolean.
    Bool(bool),
    /// An unsigned eight-bit integer.
    U8(u8),
    /// An unsigned sixteen-bit integer.
    U16(u16),
    /// An unsigned thirty-two-bit integer.
    U32(u32),
    /// An unsigned sixty-four-bit integer.
    U64(u64),
    /// A signed sixty-four-bit integer.
    I64(i64),
    /// An IEEE 754 double.
    F64(f64),
    /// A borrowed UTF-8 string.
    String(&'a str),
    /// Borrowed opaque bytes.
    Bytes(&'a [u8]),
}

impl Value<'_> {
    fn kind(self) -> Option<Kind> {
        match self {
            Self::Null => None,
            Self::Bool(_) => Some(Kind::Bool),
            Self::U8(_) => Some(Kind::U8),
            Self::U16(_) => Some(Kind::U16),
            Self::U32(_) => Some(Kind::U32),
            Self::U64(_) => Some(Kind::U64),
            Self::I64(_) => Some(Kind::I64),
            Self::F64(_) => Some(Kind::F64),
            Self::String(_) => Some(Kind::String),
            Self::Bytes(_) => Some(Kind::Bytes),
        }
    }
}

#[derive(Debug)]
struct Slot {
    field: Field,
    offset: usize,
    presence_byte: u32,
    presence_mask: u8,
}

impl Slot {
    #[inline]
    fn present(&self, bytes: &[u8]) -> bool {
        self.presence_mask == 0
            || bytes
                .get(self.presence_byte as usize)
                .is_some_and(|byte| byte & self.presence_mask != 0)
    }
}

/// A bounded schema compiled into fixed field offsets.
#[derive(Debug)]
pub struct Schema {
    slots: Vec<Slot>,
    optional: usize,
    length: usize,
}

impl Schema {
    /// Compile fields in caller-defined order, without native alignment padding.
    ///
    /// # Errors
    /// Fails on field/record limits, size overflow, or allocation failure.
    pub fn new(fields: &[Field], limits: Limits) -> Result<Self, Error> {
        if fields.len() > limits.records || fields.len() > u32::MAX as usize {
            return Err(Error::Limit("codec fields"));
        }

        let optional = fields
            .iter()
            .filter(|field| matches!(field, Field::Optional(_)))
            .count();
        let mut length = optional.div_ceil(8);
        let mut bit = 0usize;
        let mut slots = Vec::new();
        slots.try_reserve_exact(fields.len())?;

        for &field in fields {
            let (presence_byte, presence_mask) = if matches!(field, Field::Optional(_)) {
                let presence = ((bit / 8) as u32, 1 << (bit % 8));
                bit += 1;
                presence
            } else {
                (0, 0)
            };

            slots.push(Slot {
                field,
                offset: length,
                presence_byte,
                presence_mask,
            });

            if !matches!(field, Field::Absent(_)) {
                length = length
                    .checked_add(field.kind().width())
                    .ok_or(Error::Limit("codec record bytes"))?;
            }
        }

        if length > limits.bytes {
            return Err(Error::Limit("codec record bytes"));
        }

        Ok(Self {
            slots,
            optional,
            length,
        })
    }

    /// Encoded byte length of every record using this schema.
    pub fn record_length(&self) -> usize {
        self.length
    }

    /// Number of fields, including globally absent fields.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether this schema declares no fields.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Return the field declaration at a zero-based schema position.
    pub fn field(&self, position: usize) -> Option<Field> {
        self.slots.get(position).map(|slot| slot.field)
    }
}

/// Encode independent records while sharing strings and bytes across the dataset.
#[derive(Debug)]
pub struct Encoder {
    schema: Schema,
    strings: ValuePoolBuilder,
    bytes: ValuePoolBuilder,
    limits: Limits,
}

impl Encoder {
    /// Use one immutable schema for all records and bound each shared pool.
    pub fn new(schema: Schema, limits: Limits) -> Self {
        Self {
            schema,
            strings: ValuePoolBuilder::new(limits),
            bytes: ValuePoolBuilder::new(limits),
            limits,
        }
    }

    /// Encode one record. Failed encoding returns no record but may retain interned values.
    ///
    /// # Errors
    /// Fails on a missing/incorrect field, pool limits, or allocation failure.
    pub fn encode(&mut self, values: &[Value<'_>]) -> Result<Vec<u8>, Error> {
        if self.schema.slots.len() > self.limits.records {
            return Err(Error::Limit("codec fields"));
        }

        if self.schema.length > self.limits.bytes {
            return Err(Error::Limit("codec record bytes"));
        }

        if values.len() != self.schema.slots.len() {
            return Err(Error::Invalid("codec field count"));
        }

        for (position, (slot, value)) in self.schema.slots.iter().zip(values).enumerate() {
            match (slot.field, value.kind()) {
                (Field::Absent(_), None) | (Field::Optional(_), None) => {}
                (Field::Required(kind) | Field::Optional(kind), Some(actual)) if kind == actual => {
                }
                _ => {
                    return Err(Error::CodecField {
                        position,
                        reason: "unexpected or missing value",
                    });
                }
            }
        }

        let mut record = Vec::new();
        record.try_reserve_exact(self.schema.length)?;
        record.resize(self.schema.optional.div_ceil(8), 0);

        for (slot, &value) in self.schema.slots.iter().zip(values) {
            if matches!(slot.field, Field::Absent(_)) {
                continue;
            }

            if slot.presence_mask != 0 && value != Value::Null {
                *record
                    .get_mut(slot.presence_byte as usize)
                    .ok_or(Error::Invalid("codec presence bitmap"))? |= slot.presence_mask;
            }

            match value {
                Value::Null => record.resize(record.len() + slot.field.kind().width(), 0),
                Value::Bool(value) => record.push(u8::from(value)),
                Value::U8(value) => record.push(value),
                Value::U16(value) => record.extend_from_slice(&value.to_le_bytes()),
                Value::U32(value) => record.extend_from_slice(&value.to_le_bytes()),
                Value::U64(value) => record.extend_from_slice(&value.to_le_bytes()),
                Value::I64(value) => record.extend_from_slice(&value.to_le_bytes()),
                Value::F64(value) => record.extend_from_slice(&value.to_le_bytes()),
                Value::String(value) => {
                    record.extend_from_slice(&self.strings.intern(value.as_bytes())?.to_le_bytes())
                }
                Value::Bytes(value) => {
                    record.extend_from_slice(&self.bytes.intern(value)?.to_le_bytes())
                }
            }
        }

        Ok(record)
    }

    /// Finish schema and shared pools as metadata for the encoded records.
    ///
    /// # Errors
    /// Fails on metadata limits, size overflow, or allocation failure.
    pub fn into_metadata(self) -> Result<Vec<u8>, Error> {
        if self.schema.slots.len() > self.limits.records {
            return Err(Error::Limit("codec fields"));
        }

        if self.schema.length > self.limits.bytes {
            return Err(Error::Limit("codec record bytes"));
        }

        let strings = self.strings.encoded_length()?;
        let bytes = self.bytes.encoded_length()?;
        let size = self
            .schema
            .slots
            .len()
            .checked_mul(2)
            .and_then(|size| size.checked_add(HEADER + 8))
            .and_then(|size| size.checked_add(strings))
            .and_then(|size| size.checked_add(bytes))
            .ok_or(Error::Limit("codec metadata bytes"))?;

        if size > self.limits.bytes {
            return Err(Error::Limit("codec metadata bytes"));
        }

        let mut output = Vec::new();
        output.try_reserve_exact(size)?;
        output.extend_from_slice(MAGIC);
        output.extend_from_slice(&(self.schema.slots.len() as u32).to_le_bytes());

        for slot in self.schema.slots {
            output.extend_from_slice(&[slot.field.kind() as u8, slot.field.mode()]);
        }

        output.extend_from_slice(&(strings as u64).to_le_bytes());
        self.strings.write_to(&mut output)?;
        self.bytes.write_to(&mut output)?;

        Ok(output)
    }
}

/// Validated schema and shared pools borrowing immutable metadata.
#[derive(Debug)]
pub struct Decoder<'a> {
    schema: Schema,
    strings: StringPool<'a>,
    bytes: ValuePool<'a>,
}

impl<'a> Decoder<'a> {
    /// Validate metadata and every shared string once, before serving records.
    ///
    /// # Errors
    /// Fails on invalid schema/pools, UTF-8, limits, or allocation failure.
    pub fn open(metadata: &'a [u8], limits: Limits) -> Result<Self, Error> {
        if metadata.len() > limits.bytes {
            return Err(Error::Limit("codec metadata bytes"));
        }

        if metadata.get(..8) != Some(MAGIC.as_slice()) {
            return Err(Error::Invalid("codec magic"));
        }

        let count = u32::from_le_bytes(read(metadata, 8)?) as usize;
        if count > limits.records {
            return Err(Error::Limit("codec fields"));
        }

        let end = count
            .checked_mul(2)
            .and_then(|size| size.checked_add(HEADER))
            .ok_or(Error::Invalid("codec schema length"))?;
        let encoded = metadata
            .get(HEADER..end)
            .ok_or(Error::Invalid("codec schema length"))?;

        let mut fields = Vec::new();
        fields.try_reserve_exact(count)?;
        for row in encoded.chunks_exact(2) {
            let &[tag, mode] = row else {
                return Err(Error::Invalid("codec field declaration"));
            };
            let kind = Kind::decode(tag)?;
            fields.push(match mode {
                0 => Field::Required(kind),
                1 => Field::Optional(kind),
                2 => Field::Absent(kind),
                _ => return Err(Error::Invalid("codec field presence")),
            });
        }

        let schema = Schema::new(&fields, limits)?;
        drop(fields);
        let length = usize::try_from(u64::from_le_bytes(read(metadata, end)?))
            .map_err(|_| Error::Invalid("codec string pool length"))?;
        let start = end
            .checked_add(8)
            .ok_or(Error::Invalid("codec pool offset"))?;
        let split = start
            .checked_add(length)
            .ok_or(Error::Invalid("codec pool offset"))?;
        let strings = StringPool::open(
            metadata
                .get(start..split)
                .ok_or(Error::Invalid("codec string pool length"))?,
            limits,
        )?;
        let bytes = ValuePool::open(
            metadata
                .get(split..)
                .ok_or(Error::Invalid("codec byte pool length"))?,
            limits,
        )?;

        Ok(Self {
            schema,
            strings,
            bytes,
        })
    }

    /// Access the schema encoded with these pools.
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Check record length and presence padding; validate requested values on access.
    ///
    /// # Errors
    /// Fails on an incorrect length or nonzero unused presence bits.
    #[inline]
    pub fn record<'d>(&'d self, bytes: &'d [u8]) -> Result<Record<'d, 'a>, Error> {
        if bytes.len() != self.schema.length {
            return Err(Error::Invalid("codec record length"));
        }

        let tail = self.schema.optional % 8;
        if tail != 0
            && bytes
                .get(self.schema.optional / 8)
                .is_none_or(|byte| byte >> tail != 0)
        {
            return Err(Error::Invalid("codec presence padding"));
        }

        Ok(Record {
            decoder: self,
            bytes,
        })
    }
}

/// A borrowed record view. Values are checked individually rather than decoded eagerly.
///
/// Typed accessors check the declared kind and return `None` for missing fields.
/// Strings and bytes borrow metadata, not the record. Use [`Self::get`] when the
/// kind is determined at runtime, or [`Self::validate`] to check every field.
#[derive(Debug, Clone, Copy)]
pub struct Record<'d, 'a> {
    decoder: &'d Decoder<'a>,
    bytes: &'d [u8],
}

impl<'a> Record<'_, 'a> {
    /// Read a field by zero-based schema position; strings/bytes borrow metadata.
    ///
    /// # Errors
    /// Fails on an unknown position, invalid boolean, or invalid shared-value ID.
    #[inline]
    pub fn get(&self, position: usize) -> Result<Value<'a>, Error> {
        let slot = self
            .decoder
            .schema
            .slots
            .get(position)
            .ok_or(Error::CodecField {
                position,
                reason: "position outside schema",
            })?;

        if matches!(slot.field, Field::Absent(_)) {
            return Ok(Value::Null);
        }

        if !slot.present(self.bytes) {
            return Ok(Value::Null);
        }

        let offset = slot.offset;
        Ok(match slot.field.kind() {
            Kind::Bool => match read::<1>(self.bytes, offset)? {
                [0] => Value::Bool(false),
                [1] => Value::Bool(true),
                _ => return Err(Error::Invalid("codec boolean")),
            },
            Kind::U8 => {
                let [value] = read(self.bytes, offset)?;
                Value::U8(value)
            }
            Kind::U16 => Value::U16(u16::from_le_bytes(read(self.bytes, offset)?)),
            Kind::U32 => Value::U32(u32::from_le_bytes(read(self.bytes, offset)?)),
            Kind::U64 => Value::U64(u64::from_le_bytes(read(self.bytes, offset)?)),
            Kind::I64 => Value::I64(i64::from_le_bytes(read(self.bytes, offset)?)),
            Kind::F64 => Value::F64(f64::from_le_bytes(read(self.bytes, offset)?)),
            Kind::String => Value::String(
                self.decoder
                    .strings
                    .get(u32::from_le_bytes(read(self.bytes, offset)?))?,
            ),
            Kind::Bytes => Value::Bytes(
                self.decoder
                    .bytes
                    .get(u32::from_le_bytes(read(self.bytes, offset)?))?,
            ),
        })
    }

    /// Read a `u8` field.
    ///
    /// # Errors
    /// Fails on an unknown position, incorrect kind, or invalid field bytes.
    #[inline]
    pub fn u8(&self, position: usize) -> Result<Option<u8>, Error> {
        let Some(offset) = self.offset(position, Kind::U8)? else {
            return Ok(None);
        };

        let [value] = read(self.bytes, offset)?;

        Ok(Some(value))
    }

    /// Read a `u16` field.
    ///
    /// # Errors
    /// Fails on an unknown position, incorrect kind, or invalid field bytes.
    #[inline]
    pub fn u16(&self, position: usize) -> Result<Option<u16>, Error> {
        let Some(offset) = self.offset(position, Kind::U16)? else {
            return Ok(None);
        };

        Ok(Some(u16::from_le_bytes(read(self.bytes, offset)?)))
    }

    /// Read a `u32` field.
    ///
    /// # Errors
    /// Fails on an unknown position, incorrect kind, or invalid field bytes.
    #[inline]
    pub fn u32(&self, position: usize) -> Result<Option<u32>, Error> {
        let Some(offset) = self.offset(position, Kind::U32)? else {
            return Ok(None);
        };

        Ok(Some(u32::from_le_bytes(read(self.bytes, offset)?)))
    }

    /// Read a `u64` field.
    ///
    /// # Errors
    /// Fails on an unknown position, incorrect kind, or invalid field bytes.
    #[inline]
    pub fn u64(&self, position: usize) -> Result<Option<u64>, Error> {
        let Some(offset) = self.offset(position, Kind::U64)? else {
            return Ok(None);
        };

        Ok(Some(u64::from_le_bytes(read(self.bytes, offset)?)))
    }

    /// Read an `i64` field.
    ///
    /// # Errors
    /// Fails on an unknown position, incorrect kind, or invalid field bytes.
    #[inline]
    pub fn i64(&self, position: usize) -> Result<Option<i64>, Error> {
        let Some(offset) = self.offset(position, Kind::I64)? else {
            return Ok(None);
        };

        Ok(Some(i64::from_le_bytes(read(self.bytes, offset)?)))
    }

    /// Read an `f64` field.
    ///
    /// # Errors
    /// Fails on an unknown position, incorrect kind, or invalid field bytes.
    #[inline]
    pub fn f64(&self, position: usize) -> Result<Option<f64>, Error> {
        let Some(offset) = self.offset(position, Kind::F64)? else {
            return Ok(None);
        };

        Ok(Some(f64::from_le_bytes(read(self.bytes, offset)?)))
    }

    /// Borrow a string field.
    ///
    /// # Errors
    /// Fails on an unknown position, incorrect kind, or invalid shared-value ID.
    #[inline]
    pub fn string(&self, position: usize) -> Result<Option<&'a str>, Error> {
        let Some(offset) = self.offset(position, Kind::String)? else {
            return Ok(None);
        };

        Ok(Some(
            self.decoder
                .strings
                .get(u32::from_le_bytes(read(self.bytes, offset)?))?,
        ))
    }

    /// Borrow a bytes field.
    ///
    /// # Errors
    /// Fails on an unknown position, incorrect kind, or invalid shared-value ID.
    #[inline]
    pub fn bytes(&self, position: usize) -> Result<Option<&'a [u8]>, Error> {
        let Some(offset) = self.offset(position, Kind::Bytes)? else {
            return Ok(None);
        };

        Ok(Some(
            self.decoder
                .bytes
                .get(u32::from_le_bytes(read(self.bytes, offset)?))?,
        ))
    }

    /// Read a boolean field.
    ///
    /// # Errors
    /// Fails on an unknown position, incorrect kind, or invalid boolean.
    #[inline]
    pub fn boolean(&self, position: usize) -> Result<Option<bool>, Error> {
        let Some(offset) = self.offset(position, Kind::Bool)? else {
            return Ok(None);
        };

        match read::<1>(self.bytes, offset)? {
            [0] => Ok(Some(false)),
            [1] => Ok(Some(true)),
            _ => Err(Error::Invalid("codec boolean")),
        }
    }

    /// Validate all fields before retaining or publishing a record.
    ///
    /// # Errors
    /// Fails if any present field has an invalid value or reference.
    pub fn validate(&self) -> Result<(), Error> {
        for position in 0..self.decoder.schema.slots.len() {
            self.get(position)?;
        }

        Ok(())
    }

    #[inline]
    fn offset(&self, position: usize, kind: Kind) -> Result<Option<usize>, Error> {
        let slot = self
            .decoder
            .schema
            .slots
            .get(position)
            .ok_or(Error::CodecField {
                position,
                reason: "position outside schema",
            })?;

        if slot.field.kind() != kind {
            return Err(Error::CodecField {
                position,
                reason: "incorrect kind",
            });
        }

        if matches!(slot.field, Field::Absent(_)) {
            return Ok(None);
        }

        if !slot.present(self.bytes) {
            return Ok(None);
        }

        Ok(Some(slot.offset))
    }
}

fn read<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], Error> {
    let end = offset
        .checked_add(N)
        .ok_or(Error::Invalid("codec field offset"))?;
    bytes
        .get(offset..end)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(Error::Invalid("codec field bytes"))
}
