use serde::{Deserialize, Serialize};

use sp1_curves::{
    params::{NumLimbs, NumWords},
    weierstrass::{bls12_381::bls12381_decompress, secp256k1::secp256k1_decompress},
    AffinePoint, CurveType, EllipticCurve,
};
use sp1_primitives::consts::{bytes_to_words_le_vec, words_to_bytes_le_vec};
use typenum::Unsigned;

use crate::{
    events::{
        memory::{MemoryReadRecord, MemoryWriteRecord},
        LookupId, MemoryLocalEvent,
    },
    syscalls::SyscallContext,
};

/// Elliptic Curve Add Event.
///
/// This event is emitted when an elliptic curve addition operation is performed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EllipticCurveAddEvent {
    pub(crate) lookup_id: LookupId,
    /// The shard number.
    pub shard: u32,
    /// The channel number.
    pub channel: u8,
    /// The clock cycle.
    pub clk: u32,
    /// The pointer to the first point.
    pub p_ptr: u32,
    /// The first point as a list of words.
    pub p: Vec<u32>,
    /// The pointer to the second point.
    pub q_ptr: u32,
    /// The second point as a list of words.
    pub q: Vec<u32>,
    /// The memory records for the first point.
    pub p_memory_records: Vec<MemoryWriteRecord>,
    /// The memory records for the second point.
    pub q_memory_records: Vec<MemoryReadRecord>,
    /// The local memory access records.
    pub local_mem_access: Vec<MemoryLocalEvent>,
}

/// Elliptic Curve Double Event.
///
/// This event is emitted when an elliptic curve doubling operation is performed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EllipticCurveDoubleEvent {
    /// The lookup identifer.
    pub lookup_id: LookupId,
    /// The shard number.
    pub shard: u32,
    /// The channel number.
    pub channel: u8,
    /// The clock cycle.
    pub clk: u32,
    /// The pointer to the point.
    pub p_ptr: u32,
    /// The point as a list of words.
    pub p: Vec<u32>,
    /// The memory records for the point.
    pub p_memory_records: Vec<MemoryWriteRecord>,
    /// The local memory access records.
    pub local_mem_access: Vec<MemoryLocalEvent>,
}

/// Elliptic Curve Point Decompress Event.
///
/// This event is emitted when an elliptic curve point decompression operation is performed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EllipticCurveDecompressEvent {
    /// The lookup identifer.
    pub lookup_id: LookupId,
    /// The shard number.
    pub shard: u32,
    /// The channel number.
    pub channel: u8,
    /// The clock cycle.
    pub clk: u32,
    /// The pointer to the point.
    pub ptr: u32,
    /// The sign bit of the point.
    pub sign_bit: bool,
    /// The x coordinate as a list of bytes.
    pub x_bytes: Vec<u8>,
    /// The decompressed y coordinate as a list of bytes.
    pub decompressed_y_bytes: Vec<u8>,
    /// The memory records for the x coordinate.
    pub x_memory_records: Vec<MemoryReadRecord>,
    /// The memory records for the y coordinate.
    pub y_memory_records: Vec<MemoryWriteRecord>,
    /// The local memory access records.
    pub local_mem_access: Vec<MemoryLocalEvent>,
}

/// Create an elliptic curve add event. It takes two pointers to memory locations, reads the points
/// from memory, adds them together, and writes the result back to the first memory location.
/// The generic parameter `N` is the number of u32 words in the point representation. For example,
/// for the secp256k1 curve, `N` would be 16 (64 bytes) because the x and y coordinates are 32 bytes
/// each.
pub fn create_ec_add_event<E: EllipticCurve>(
    rt: &mut SyscallContext,
    arg1: u32,
    arg2: u32,
) -> EllipticCurveAddEvent {
    let start_clk = rt.clk;
    let p_ptr = arg1;
    if p_ptr % 4 != 0 {
        panic!();
    }
    let q_ptr = arg2;
    if q_ptr % 4 != 0 {
        panic!();
    }

    let num_words = <E::BaseField as NumWords>::WordsCurvePoint::USIZE;

    let p = rt.slice_unsafe(p_ptr, num_words);

    for i in 0..num_words {
        let addr = q_ptr + i as u32 * 4;
        let local_mem_access = rt.rt.local_memory_access.remove(&addr);

        if let Some(local_mem_access) = local_mem_access {
            rt.rt.record.local_memory_access.push(local_mem_access);
        }
    }

    let (q_memory_records, q) = rt.mr_slice(q_ptr, num_words);

    let mut ec_add_local_mem_access = Vec::new();
    for i in 0..num_words {
        let addr = q_ptr + i as u32 * 4;
        let local_mem_access =
            rt.rt.local_memory_access.remove(&addr).expect("Expected local memory access");

        ec_add_local_mem_access.push(local_mem_access);
    }

    // When we write to p, we want the clk to be incremented because p and q could be the same.
    rt.clk += 1;

    let p_affine = AffinePoint::<E>::from_words_le(&p);
    let q_affine = AffinePoint::<E>::from_words_le(&q);
    let result_affine = p_affine + q_affine;

    let result_words = result_affine.to_words_le();

    for i in 0..result_words.len() {
        let addr = p_ptr + i as u32 * 4;
        let local_mem_access = rt.rt.local_memory_access.remove(&addr);

        if let Some(local_mem_access) = local_mem_access {
            rt.rt.record.local_memory_access.push(local_mem_access);
        }
    }

    let p_memory_records = rt.mw_slice(p_ptr, &result_words);

    for i in 0..result_words.len() {
        let addr = p_ptr + i as u32 * 4;
        let local_mem_access =
            rt.rt.local_memory_access.remove(&addr).expect("Expected local memory access");

        ec_add_local_mem_access.push(local_mem_access);
    }

    EllipticCurveAddEvent {
        lookup_id: rt.syscall_lookup_id,
        shard: rt.current_shard(),
        channel: rt.current_channel(),
        clk: start_clk,
        p_ptr,
        p,
        q_ptr,
        q,
        p_memory_records,
        q_memory_records,
        local_mem_access: ec_add_local_mem_access,
    }
}

/// Create an elliptic curve double event.
///
/// It takes a pointer to a memory location, reads the point from memory, doubles it, and writes the
/// result back to the memory location.
pub fn create_ec_double_event<E: EllipticCurve>(
    rt: &mut SyscallContext,
    arg1: u32,
    _: u32,
) -> EllipticCurveDoubleEvent {
    let start_clk = rt.clk;
    let p_ptr = arg1;
    if p_ptr % 4 != 0 {
        panic!();
    }

    let num_words = <E::BaseField as NumWords>::WordsCurvePoint::USIZE;

    let p = rt.slice_unsafe(p_ptr, num_words);

    let p_affine = AffinePoint::<E>::from_words_le(&p);

    let result_affine = E::ec_double(&p_affine);

    let result_words = result_affine.to_words_le();

    for i in 0..result_words.len() {
        let addr = p_ptr + i as u32 * 4;
        let local_mem_access = rt.rt.local_memory_access.remove(&addr);

        if let Some(local_mem_access) = local_mem_access {
            rt.rt.record.local_memory_access.push(local_mem_access);
        }
    }

    let p_memory_records = rt.mw_slice(p_ptr, &result_words);

    let mut ec_double_local_mem_access = Vec::new();
    for i in 0..result_words.len() {
        let addr = p_ptr + i as u32 * 4;
        let local_mem_access =
            rt.rt.local_memory_access.remove(&addr).expect("Expected local memory access");

        ec_double_local_mem_access.push(local_mem_access);
    }

    EllipticCurveDoubleEvent {
        lookup_id: rt.syscall_lookup_id,
        shard: rt.current_shard(),
        channel: rt.current_channel(),
        clk: start_clk,
        p_ptr,
        p,
        p_memory_records,
        local_mem_access: ec_double_local_mem_access,
    }
}

/// Create an elliptic curve decompress event.
///
/// It takes a pointer to a memory location, reads the point from memory, decompresses it, and
/// writes the result back to the memory location.
pub fn create_ec_decompress_event<E: EllipticCurve>(
    rt: &mut SyscallContext,
    slice_ptr: u32,
    sign_bit: u32,
) -> EllipticCurveDecompressEvent {
    let start_clk = rt.clk;
    assert!(slice_ptr % 4 == 0, "slice_ptr must be 4-byte aligned");
    assert!(sign_bit <= 1, "is_odd must be 0 or 1");

    let num_limbs = <E::BaseField as NumLimbs>::Limbs::USIZE;
    let num_words_field_element = num_limbs / 4;

    for i in 0..num_words_field_element {
        let addr = (slice_ptr + (num_limbs as u32)) + i as u32 * 4;
        let local_mem_access = rt.rt.local_memory_access.remove(&addr);

        if let Some(local_mem_access) = local_mem_access {
            rt.rt.record.local_memory_access.push(local_mem_access);
        }
    }

    let (x_memory_records, x_vec) =
        rt.mr_slice(slice_ptr + (num_limbs as u32), num_words_field_element);

    let mut ec_decompress_local_mem_access = Vec::new();
    for i in 0..num_words_field_element {
        let addr = (slice_ptr + (num_limbs as u32)) + i as u32 * 4;
        let local_mem_access =
            rt.rt.local_memory_access.remove(&addr).expect("Expected local memory access");

        ec_decompress_local_mem_access.push(local_mem_access);
    }

    let x_bytes = words_to_bytes_le_vec(&x_vec);
    let mut x_bytes_be = x_bytes.clone();
    x_bytes_be.reverse();

    let decompress_fn = match E::CURVE_TYPE {
        CurveType::Secp256k1 => secp256k1_decompress::<E>,
        CurveType::Bls12381 => bls12381_decompress::<E>,
        _ => panic!("Unsupported curve"),
    };

    let computed_point: AffinePoint<E> = decompress_fn(&x_bytes_be, sign_bit);

    let mut decompressed_y_bytes = computed_point.y.to_bytes_le();
    decompressed_y_bytes.resize(num_limbs, 0u8);
    let y_words = bytes_to_words_le_vec(&decompressed_y_bytes);

    for i in 0..y_words.len() {
        let addr = slice_ptr + i as u32 * 4;
        let local_mem_access = rt.rt.local_memory_access.remove(&addr);

        if let Some(local_mem_access) = local_mem_access {
            rt.rt.record.local_memory_access.push(local_mem_access);
        }
    }
    let y_memory_records = rt.mw_slice(slice_ptr, &y_words);
    for i in 0..y_words.len() {
        let addr = slice_ptr + i as u32 * 4;
        let local_mem_access =
            rt.rt.local_memory_access.remove(&addr).expect("Expected local memory access");

        ec_decompress_local_mem_access.push(local_mem_access);
    }

    EllipticCurveDecompressEvent {
        lookup_id: rt.syscall_lookup_id,
        shard: rt.current_shard(),
        channel: rt.current_channel(),
        clk: start_clk,
        ptr: slice_ptr,
        sign_bit: sign_bit != 0,
        x_bytes: x_bytes.clone(),
        decompressed_y_bytes,
        x_memory_records,
        y_memory_records,
        local_mem_access: ec_decompress_local_mem_access,
    }
}
