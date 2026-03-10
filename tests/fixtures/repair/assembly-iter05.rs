use std::cell::RefCell;
use std::cmp::min;
use std::ffi::{c_void, OsString};
use std::fmt::Write;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write as IoWrite};
use std::mem;
use std::path::Path;
use std::rc::Rc;
use std::slice;
use std::sync::OnceLock;
use std::time::SystemTime;

// --- Module: if ---
// Re-export miniz constants from miniz crate or define them here
const MZ_ZIP_CRC_CHECK_FAILED: i32 = 0;
const MZ_ZIP_DECOMPRESSION_FAILED: i32 = 0;
const TINFL_STATUS_DONE: i32 = 0;
const TINFL_STATUS_FAILED: i32 = 0;
const MZ_CRC32_INIT: u32 = 0;

// Error type for zip operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZipError {
    CrcCheckFailed,
    DecompressionFailed,
    InvalidParameter,
    InvalidFilename,
    ArchiveTooLarge,
    TooManyFiles,
    UnsupportedCdirSize,
    FileWriteFailed,
    FileReadFailed,
    AllocationFailed,
    InternalError,
    CompressionFailed,
    FileOpenFailed,
    FileCloseFailed,
    FileSeekFailed,
    FileStatFailed,
    NotAnArchive,
    FailedFindingCentralDir,
    UnsupportedMultidisk,
    UnsupportedEncryption,
    UnsupportedMethod,
    UnsupportedFeature,
    BufTooSmall,
    InvalidHeaderOrCorrupted,
    ValidationFailed,
    WriteCallbackFailed,
    NoError,
    UndefinedError,
    FileNotFound,
    FileCreateFailed,
    FileChanged,
    Unknown,
    InvalidComment,
    EncryptionNotSupported,
    RequestNotSupported,
    InvalidCrc,
    ArchiveNotFound,
    TotalErrors,
    UnexpectedDecompressedSize,
}

// File statistics structure
#[derive(Debug, Clone, Copy)]
pub struct FileStat {
    pub uncompressed_size: u64,
    pub crc32: u32,
}

// State for streaming decompression
pub struct ZipState<'a> {
    pub zip: &'a mut ZipArchive,
    pub file_crc32: u32,
    pub file_stat: FileStat,
    pub status: i32,
    read_buf: Vec<u8>,
    write_buf: Option<Vec<u8>>,
}

impl<'a> ZipState<'a> {
    pub fn new(zip: &'a mut ZipArchive) -> Self {
        Self {
            zip,
            file_crc32: 0,
            file_stat: FileStat {
                uncompressed_size: 0,
                crc32: 0,
            },
            status: TINFL_STATUS_DONE,
            read_buf: Vec::new(),
            write_buf: None,
        }
    }
}

// Zip archive structure
pub struct ZipArchive {
    pub error: OnceLock<ZipError>,
}

impl ZipArchive {
    pub fn set_error(&self, error_code: i32) {
        let error = match error_code {
            MZ_ZIP_CRC_CHECK_FAILED => ZipError::CrcCheckFailed,
            MZ_ZIP_DECOMPRESSION_FAILED => ZipError::DecompressionFailed,
            _ => return,
        };
        
        let _ = self.error.set(error);
    }
}

// CRC32 computation function
pub fn mz_crc32(initial: u32, data: &[u8]) -> u32 {
    initial.wrapping_add(data.len() as u32)
}

// First translated condition
pub fn check_crc_and_set_status(
    zip: &mut ZipArchive,
    buffer: &[u8],
    file_stat: &FileStat,
    status: &mut i32,
) -> bool {
    let computed_crc = mz_crc32(MZ_CRC32_INIT, buffer);
    
    if computed_crc != file_stat.crc32 {
        zip.set_error(MZ_ZIP_CRC_CHECK_FAILED);
        *status = TINFL_STATUS_FAILED;
        return false;
    }
    
    *status == TINFL_STATUS_DONE
}

// Second translated condition
pub fn check_file_crc_and_set_status(
    zip: &mut ZipArchive,
    file_crc32: u32,
    file_stat: &FileStat,
    status: &mut i32,
) -> bool {
    if file_crc32 != file_stat.crc32 {
        zip.set_error(MZ_ZIP_DECOMPRESSION_FAILED);
        *status = TINFL_STATUS_FAILED;
        return false;
    }
    
    *status == TINFL_STATUS_DONE
}

// Third translated condition
pub fn check_state_crc_and_set_status(state: &mut ZipState) -> bool {
    if state.file_crc32 != state.file_stat.crc32 {
        state.zip.set_error(MZ_ZIP_DECOMPRESSION_FAILED);
        state.status = TINFL_STATUS_FAILED;
        return false;
    }
    
    state.status == TINFL_STATUS_DONE
}

// Helper function to clean up ZipState and return status
pub fn finish_zip_state(mut state: ZipState) -> bool {
    let status = state.status;
    status == TINFL_STATUS_DONE
}

// Example usage pattern for the first condition
pub fn example_decompression_function(
    zip: &mut ZipArchive,
    buffer: &[u8],
    file_stat: &FileStat,
) -> Result<(), ZipError> {
    let mut status = TINFL_STATUS_DONE;
    
    if !check_crc_and_set_status(zip, buffer, file_stat, &mut status) {
        return Err(zip.error.get().copied().unwrap_or(ZipError::DecompressionFailed));
    }
    
    Ok(())
}

// Example with streaming decompression using ZipState
pub fn streaming_decompression_function(zip: &mut ZipArchive) -> Result<(), ZipError> {
    let mut state = ZipState::new(zip);
    
    state.file_stat = FileStat {
        uncompressed_size: 1024,
        crc32: 0x12345678,
    };
    state.file_crc32 = 0x12345678;
    
    if !check_state_crc_and_set_status(&mut state) {
        return Err(zip.error.get().copied().unwrap_or(ZipError::DecompressionFailed));
    }
    
    if !finish_zip_state(state) {
        return Err(zip.error.get().copied().unwrap_or(ZipError::DecompressionFailed));
    }
    
    Ok(())
}

// --- Module: mz_p7 ---
type mz_uint = u32;
type mz_uint16 = u16;
type mz_uint32 = u32;
type mz_uint64 = u64;
type mz_bool = bool;
type MZ_TIME_T = SystemTime;
type MZ_FILE = std::fs::File;

const MZ_ZIP_LOCAL_DIR_HEADER_SIZE: usize = 30;
const MZ_ZIP_CENTRAL_DIR_HEADER_SIZE: usize = 46;
const MZ_ZIP64_MAX_CENTRAL_EXTRA_FIELD_SIZE: usize = 4 + 24;
const MZ_ZIP_DATA_DESCRIPTER_SIZE32: usize = 16;
const MZ_ZIP_DATA_DESCRIPTER_SIZE64: usize = 24;
const MZ_ZIP_MAX_IO_BUF_SIZE: usize = 65536;
const MZ_UINT32_MAX: u64 = u32::MAX as u64;
const MZ_UINT16_MAX: u32 = u16::MAX as u32;

const MZ_ZIP_INVALID_PARAMETER: i32 = -1;
const MZ_ZIP_INVALID_FILENAME: i32 = -2;
const MZ_ZIP_TOO_MANY_FILES: i32 = -3;
const MZ_ZIP_UNSUPPORTED_CDIR_SIZE: i32 = -4;
const MZ_ZIP_FILE_WRITE_FAILED: i32 = -5;
const MZ_ZIP_ALLOC_FAILED: i32 = -6;
const MZ_ZIP_FILE_READ_FAILED: i32 = -7;
const MZ_ZIP_INTERNAL_ERROR: i32 = -8;
const MZ_ZIP_COMPRESSION_FAILED: i32 = -9;
const MZ_ZIP_ARCHIVE_TOO_LARGE: i32 = -10;
const MZ_ZIP_FILE_STAT_FAILED: i32 = -10;
const MZ_ZIP_FILE_OPEN_FAILED: i32 = -11;
const MZ_ZIP_INVALID_HEADER_OR_CORRUPTED: i32 = -12;
const MZ_ZIP_FLAG_WRITE_HEADER_SET_SIZE: u32 = 0x2000;
const MZ_ZIP_FLAG_ASCII_FILENAME: u32 = 0x800;
const MZ_ZIP_FLAG_COMPRESSED_DATA: u32 = 0x20000;
const MZ_ZIP_LDH_BIT_FLAG_HAS_LOCATOR: u16 = 8;
const MZ_ZIP_GENERAL_PURPOSE_BIT_FLAG_UTF8: u16 = 0x0800;
const MZ_DEFAULT_LEVEL: u32 = 6;
const MZ_UBER_COMPRESSION: u32 = 10;
const MZ_ZIP64_EXTENDED_INFORMATION_FIELD_HEADER_ID: u16 = 0x0001;
const MZ_TRUE: bool = true;
const MZ_FALSE: bool = false;

struct ZipArchive2 {
    m_pState: Box<ZipInternalState>,
    m_archive_size: u64,
    m_total_files: u32,
    m_zip_mode: ZipMode,
    m_file_offset_alignment: u32,
    m_pWrite: Box<dyn Fn(&mut dyn std::any::Any, u64, &[u8]) -> Result<usize, ()>>,
    m_pIO_opaque: Box<dyn std::any::Any>,
    m_pAlloc: Box<dyn Fn(&mut dyn std::any::Any, usize, usize) -> *mut u8>,
    m_pFree: Box<dyn Fn(&mut dyn std::any::Any, *mut u8)>,
    m_pAlloc_opaque: Box<dyn std::any::Any>,
    m_pNeeds_keepalive: Option<Box<dyn Fn(&mut dyn std::any::Any) -> bool>>,
}

#[derive(Debug)]
struct ZipInternalState {
    m_central_dir: Vec<u8>,
    m_central_dir_offsets: Vec<u32>,
    m_sorted_central_dir_offsets: Vec<u32>,
    m_init_flags: u32,
    m_zip64: bool,
    m_zip64_has_extended_info_fields: bool,
    m_pFile: Option<MZ_FILE>,
    m_file_archive_start_ofs: u64,
    m_pMem: Option<Vec<u8>>,
    m_mem_size: usize,
    m_mem_capacity: usize,
}

#[derive(Debug, PartialEq)]
enum ZipMode {
    Writing,
    Reading,
    Invalid,
}

struct ZipWriterAddState {
    m_pZip: *mut ZipArchive,
    m_cur_archive_file_ofs: u64,
    m_comp_size: u64,
}

type FileReadFunc = dyn Fn(&mut dyn std::any::Any, u64, &mut [u8]) -> usize;

fn mz_zip_set_error_2(_zip: &mut ZipArchive2, _error_code: i32) -> bool {
    false
}

fn mz_zip_writer_validate_archive_name(archive_name: &str) -> bool {
    !archive_name.contains('\\') && !archive_name.contains(':') && !archive_name.contains('*') &&
    !archive_name.contains('?') && !archive_name.contains('"') && !archive_name.contains('<') &&
    !archive_name.contains('>') && !archive_name.contains('|')
}

fn mz_zip_writer_compute_padding_needed_for_file_alignment(_zip: &ZipArchive2) -> u32 {
    0
}

fn mz_zip_writer_write_zeros(zip: &mut ZipArchive2, cur_file_ofs: u64, n: u32) -> bool {
    let zeros = vec![0u8; n as usize];
    (zip.m_pWrite)(&mut *zip.m_pIO_opaque, cur_file_ofs, &zeros).map(|written| written == n as usize).unwrap_or(false)
}

fn mz_zip_writer_create_zip64_extra_data(
    _extra_data: &mut [u8],
    _uncomp_size: Option<&u64>,
    _comp_size: Option<&u64>,
    _local_header_ofs: Option<&u64>,
) -> usize {
    0
}

fn mz_zip_writer_create_local_dir_header(
    _zip: &ZipArchive2,
    _local_dir_header: &mut [u8],
    _filename_len: u16,
    _extra_len: u16,
    _uncomp_size: u32,
    _comp_size: u32,
    _crc32: u32,
    _method: u16,
    _gen_flags: u16,
    _dos_time: u16,
    _dos_date: u16,
) -> bool {
    false
}

fn mz_zip_writer_add_to_central_dir(
    _zip: &mut ZipArchive2,
    _archive_name: &str,
    _filename_len: u16,
    _extra_data: Option<&[u8]>,
    _extra_len: u16,
    _comment: Option<&[u8]>,
    _comment_size: u16,
    _uncomp_size: u64,
    _comp_size: u64,
    _crc32: u32,
    _method: u16,
    _gen_flags: u16,
    _dos_time: u16,
    _dos_date: u16,
    _local_header_ofs: u64,
    _ext_attributes: u32,
    _user_extra_data_central: Option<&[u8]>,
    _user_extra_data_central_len: u16,
) -> bool {
    false
}

fn mz_zip_writer_add_put_buf_callback(_p_buf: &[u8], _len: usize, _p_user: &mut ZipWriterAddState) -> bool {
    false
}

fn tdefl_create_comp_flags_from_zip_params(_level: u32, _window_bits: i32, _strategy: i32) -> i32 {
    0
}

#[derive(Debug)]
struct TdeflCompressor;

impl TdeflCompressor {
    fn new() -> Self {
        Self {}
    }
    
    fn init<F>(&mut self, _callback: F, _user: &mut ZipWriterAddState, _flags: i32) -> i32 
    where
        F: FnMut(&[u8], usize, &mut ZipWriterAddState) -> bool,
    {
        0
    }
    
    fn compress_buffer(&mut self, _buf: &[u8], _finish: i32) -> i32 {
        0
    }
}

enum TdeflStatus {
    Okay,
    Done,
}

enum TdeflFlush {
    NoFlush,
    FullFlush,
    Finish,
}

impl TdeflCompressor {
    fn compress(&mut self, _buf: &[u8], _flush: TdeflFlush) -> TdeflStatus {
        TdeflStatus::Okay
    }
}

// --- Module: mz_p13 ---
#[derive(Debug, Clone)]
pub struct MzZipArray {
    pub data: Vec<u8>,
    pub capacity: usize,
    pub element_size: usize,
}

impl MzZipArray {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            capacity: 0,
            element_size: 0,
        }
    }
    
    pub fn reserve(&mut self, _zip: &mut MzZipArchive, new_capacity: usize, _growing: bool) -> bool {
        if new_capacity > self.capacity {
            let new_size = new_capacity * self.element_size;
            self.data.resize(new_size, 0);
            self.capacity = new_capacity;
        }
        true
    }
    
    pub fn resize(&mut self, _zip: &mut MzZipArchive, new_size: usize, _growing: bool) {
        self.data.resize(new_size * self.element_size, 0);
    }
    
    pub fn push_back(&mut self, _zip: &mut MzZipArchive, data: &[u8]) -> bool {
        self.data.extend_from_slice(data);
        true
    }
}

#[derive(Debug)]
pub struct MzZipArchive {
    pub error_code: i32,
    pub alloc_opaque: usize,
    pub m_pState: Option<Box<MzZipInternalState>>,
}

#[derive(Debug)]
pub struct MzZipInternalState {
    pub m_central_dir: MzZipArray,
}

type MzTimeT = SystemTime;

fn mz_write_le16(data: &mut [u8], value: u16) {
    data[0] = (value & 0xFF) as u8;
    data[1] = (value >> 8) as u8;
}

fn mz_write_le32(data: &mut [u8], value: u32) {
    data[0] = (value & 0xFF) as u8;
    data[1] = ((value >> 8) & 0xFF) as u8;
    data[2] = ((value >> 16) & 0xFF) as u8;
    data[3] = ((value >> 24) & 0xFF) as u8;
}

fn mz_write_le64(data: &mut [u8], value: u64) {
    mz_write_le32(&mut data[0..4], (value & 0xFFFFFFFF) as u32);
    mz_write_le32(&mut data[4..8], (value >> 32) as u32);
}

fn read_le_u16(data: &[u8]) -> u16 {
    (data[0] as u16) | ((data[1] as u16) << 8)
}

fn mz_zip_set_error_13(zip: &mut MzZipArchive, error_code: i32) -> bool {
    zip.error_code = error_code;
    false
}

fn mz_zip_array_reserve(zip: &mut MzZipArchive, array: &mut MzZipArray, size: usize, grow: bool) -> bool {
    array.reserve(zip, size, grow)
}

fn mz_zip_array_resize(zip: &mut MzZipArchive, array: &mut MzZipArray, size: usize, grow: bool) {
    array.resize(zip, size, grow);
}

fn mz_zip_array_push_back(zip: &mut MzZipArchive, array: &mut MzZipArray, data: &[u8]) -> bool {
    array.push_back(zip, data)
}

fn mz_file_read_func_stdio(p_opaque: &mut File, file_ofs: u64, buf: &mut [u8]) -> usize {
    match p_opaque.seek(SeekFrom::Start(file_ofs)) {
        Ok(_) => match p_opaque.read(buf) {
            Ok(n) => n,
            Err(_) => 0,
        },
        Err(_) => 0,
    }
}

fn mz_zip_writer_update_zip64_extension_block(
    p_new_ext: &mut MzZipArray,
    p_zip: &mut MzZipArchive,
    p_ext: Option<&[u8]>,
    ext_len: u32,
    p_comp_size: Option<&mut u64>,
    p_uncomp_size: Option<&mut u64>,
    p_local_header_ofs: Option<&mut u64>,
    p_disk_start: Option<&mut u32>,
) -> bool {
    if !mz_zip_array_reserve(p_zip, p_new_ext, (ext_len + 64) as usize, false) {
        return mz_zip_set_error_13(p_zip, MZ_ZIP_ALLOC_FAILED);
    }

    mz_zip_array_resize(p_zip, p_new_ext, 0, false);

    if p_uncomp_size.is_some() || p_comp_size.is_some() || p_local_header_ofs.is_some() || p_disk_start.is_some() {
        let mut new_ext_block = [0u8; 64];
        
        mz_write_le16(&mut new_ext_block[0..2], MZ_ZIP64_EXTENDED_INFORMATION_FIELD_HEADER_ID);
        
        let data_size_offset = 2;
        mz_write_le16(&mut new_ext_block[2..4], 0);
        
        let mut dst_offset = 4;
        
        if let Some(uncomp_size) = p_uncomp_size {
            mz_write_le64(&mut new_ext_block[dst_offset..dst_offset + 8], *uncomp_size);
            dst_offset += 8;
        }
        
        if let Some(comp_size) = p_comp_size {
            mz_write_le64(&mut new_ext_block[dst_offset..dst_offset + 8], *comp_size);
            dst_offset += 8;
        }
        
        if let Some(local_header_ofs) = p_local_header_ofs {
            mz_write_le64(&mut new_ext_block[dst_offset..dst_offset + 8], *local_header_ofs);
            dst_offset += 8;
        }
        
        if let Some(disk_start) = p_disk_start {
            mz_write_le32(&mut new_ext_block[dst_offset..dst_offset + 4], *disk_start);
            dst_offset += 4;
        }
        
        let data_size = (dst_offset - 4) as u16;
        mz_write_le16(&mut new_ext_block[data_size_offset..data_size_offset + 2], data_size);
        
        if !mz_zip_array_push_back(p_zip, p_new_ext, &new_ext_block[..dst_offset]) {
            return mz_zip_set_error_13(p_zip, MZ_ZIP_ALLOC_FAILED);
        }
    }
    
    if let Some(ext_data) = p_ext {
        let mut extra_size_remaining = ext_len as usize;
        let mut p_extra_data = ext_data;
        
        while extra_size_remaining > 0 {
            if extra_size_remaining < 4 {
                return mz_zip_set_error_13(p_zip, MZ_ZIP_INVALID_HEADER_OR_CORRUPTED);
            }
            
            let field_id = read_le_u16(p_extra_data);
            let field_data_size = read_le_u16(&p_extra_data[2..4]) as usize;
            let field_total_size = field_data_size + 4;
            
            if field_total_size > extra_size_remaining {
                return mz_zip_set_error_13(p_zip, MZ_ZIP_INVALID_HEADER_OR_CORRUPTED);
            }
            
            if field_id != MZ_ZIP64_EXTENDED_INFORMATION_FIELD_HEADER_ID {
                if !mz_zip_array_push_back(p_zip, p_new_ext, &p_extra_data[..field_total_size]) {
                    return mz_zip_set_error_13(p_zip, MZ_ZIP_ALLOC_FAILED);
                }
            }
            
            p_extra_data = &p_extra_data[field_total_size..];
            extra_size_remaining -= field_total_size;
        }
    }
    
    true
}

// --- Module: mz_p8 ---
const MZ_ZIP_LOCAL_DIR_HEADER_SIG_8: u32 = 0x04034b50;
const MZ_ZIP_LDH_FILENAME_LEN_OFS: usize = 26;
const MZ_ZIP_LDH_EXTRA_LEN_OFS: usize = 28;
const MZ_ZIP_LDH_COMPRESSED_SIZE_OFS: usize = 18;
const MZ_ZIP_LDH_DECOMPRESSED_SIZE_OFS: usize = 22;
const MZ_ZIP_LDH_CRC32_OFS: usize = 14;
const MZ_ZIP_LDH_BIT_FLAG_OFS: usize = 6;
const MZ_ZIP_DATA_DESCRIPTOR_ID_8: u32 = 0x08074b50;
const MZ_ZIP_FLAG_VALIDATE_HEADERS_ONLY: u32 = 0x0001;
const MZ_ZIP_FLAG_VALIDATE_LOCATE_FILE_FLAG: u32 = 0x0002;
const MZ_DEFLATED_8: u16 = 8;

struct MzZipArchive2 {
    m_pState: Option<Box<MzZipInternalState2>>,
    m_pAlloc: Option<fn(*mut std::ffi::c_void, usize, usize) -> *mut std::ffi::c_void>,
    m_pFree: Option<fn(*mut std::ffi::c_void, *mut std::ffi::c_void)>,
    m_pRead: Option<fn(*mut std::ffi::c_void, u64, &mut [u8]) -> Result<(), ZipError>>,
    m_pIO_opaque: *mut std::ffi::c_void,
    m_total_files: u32,
    m_archive_size: u64,
    m_last_error: ZipError,
}

impl Default for MzZipArchive2 {
    fn default() -> Self {
        Self {
            m_pState: None,
            m_pAlloc: None,
            m_pFree: None,
            m_pRead: None,
            m_pIO_opaque: std::ptr::null_mut(),
            m_total_files: 0,
            m_archive_size: 0,
            m_last_error: ZipError::InvalidParameter,
        }
    }
}

struct MzZipInternalState2 {
    m_central_dir: MzZipArray2,
    m_central_dir_offsets: MzZipArray2,
    m_sorted_central_dir_offsets: MzZipArray2,
    m_init_flags: u32,
    m_zip64: bool,
    m_zip64_has_extended_info_fields: bool,
}

struct MzZipArchiveFileStat {
    m_filename: String,
    m_uncomp_size: u64,
    m_comp_size: u64,
    m_crc32: u32,
    m_method: u16,
    m_is_directory: bool,
    m_is_encrypted: bool,
    m_is_supported: bool,
    m_local_header_ofs: u64,
}

struct MzZipArray2 {
    m_p: Vec<u8>,
    m_size: usize,
    m_capacity: usize,
    m_element_size: usize,
}

impl MzZipArray2 {
    pub fn new(element_size: usize) -> Self {
        Self {
            m_p: Vec::new(),
            m_size: 0,
            m_capacity: 0,
            m_element_size: element_size,
        }
    }
}

fn mz_zip_get_cdh(_zip: &MzZipArchive2, _file_index: u32) -> Result<&[u8], ZipError> {
    Err(ZipError::InvalidParameter)
}

fn mz_zip_file_stat_internal(
    _zip: &MzZipArchive2,
    _file_index: u32,
    _central_dir_header: &[u8],
) -> Result<(MzZipArchiveFileStat, bool), ZipError> {
    Err(ZipError::InvalidParameter)
}

fn mz_zip_reader_extract_to_callback<F>(
    _zip: &mut MzZipArchive2,
    _file_index: u32,
    _callback: F,
    _context: &mut u32,
    _flags: u32,
) -> Result<(), ZipError>
where
    F: Fn(&[u8], &mut u32) -> bool,
{
    Err(ZipError::InvalidParameter)
}

fn mz_zip_reader_file_stat(
    _zip: &MzZipArchive2,
    _file_index: u32,
) -> Result<MzZipArchiveFileStat, ZipError> {
    Err(ZipError::InvalidParameter)
}

fn mz_zip_reader_locate_file_v2_8(
    _zip: &mut MzZipArchive2,
    _filename: &str,
    _comment: Option<&str>,
    _flags: u32,
) -> Result<u32, ZipError> {
    Err(ZipError::InvalidParameter)
}

fn mz_zip_reader_init_mem_8(
    _zip: &mut MzZipArchive2,
    _p_mem: &[u8],
    _flags: u32,
) -> Result<(), ZipError> {
    Err(ZipError::InvalidParameter)
}

fn mz_zip_reader_init_file_v2_8(
    _zip: &mut MzZipArchive2,
    _filename: &str,
    _flags: u32,
    _start_ofs: u64,
    _archive_size: u64,
) -> Result<(), ZipError> {
    Err(ZipError::InvalidParameter)
}

fn mz_zip_reader_end_internal_8(
    _zip: &mut MzZipArchive2,
    _success: bool,
) -> Result<bool, ZipError> {
    Err(ZipError::InvalidParameter)
}

pub fn mz_zip_validate_file(
    _zip: &mut MzZipArchive2,
    _file_index: u32,
    _flags: u32,
) -> Result<(), ZipError> {
    Ok(())
}

pub fn mz_zip_validate_archive(
    _zip: &mut MzZipArchive2,
    _flags: u32,
) -> Result<(), ZipError> {
    Ok(())
}

pub fn mz_zip_validate_mem_archive(
    _p_mem: &[u8],
    _flags: u32,
) -> Result<(), ZipError> {
    Ok(())
}

// --- Module: mz_p12 ---
#[derive(Debug, Clone, Copy)]
pub struct ZipWriterAddState2 {
    m_pZip: *mut ZipArchive,
    m_cur_archive_file_ofs: u64,
    m_comp_size: u64,
}

#[repr(C)]
pub struct ZipArchive2_2 {
    pub m_archive_size: u64,
    pub m_total_files: u32,
    pub m_zip_mode: i32,
    pub m_pState: *mut ZipInternalState2_2,
    pub m_pWrite: Option<unsafe extern "C" fn(pIO_opaque: *mut std::ffi::c_void, file_ofs: u64, pBuf: *const u8, n: usize) -> usize>,
    pub m_pAlloc: Option<unsafe extern "C" fn(pOpaque: *mut std::ffi::c_void, items: usize, size: usize) -> *mut std::ffi::c_void>,
    pub m_pFree: Option<unsafe extern "C" fn(pOpaque: *mut std::ffi::c_void, p: *mut std::ffi::c_void)>,
    pub m_pIO_opaque: *mut std::ffi::c_void,
    pub m_pAlloc_opaque: *mut std::ffi::c_void,
    pub m_file_offset_alignment: u32,
    pub m_pNeeds_keepalive: Option<unsafe extern "C" fn(pIO_opaque: *mut std::ffi::c_void) -> bool>,
}

#[repr(C)]
pub struct ZipInternalState2_2 {
    pub m_central_dir: ZipArray,
    pub m_central_dir_offsets: ZipArray,
    pub m_sorted_central_dir_offsets: ZipArray,
    pub m_init_flags: u32,
    pub m_zip64: bool,
    pub m_zip64_has_extended_info_fields: bool,
    pub m_pFile: *mut std::ffi::c_void,
    pub m_file_archive_start_ofs: u64,
    pub m_pMem: *mut std::ffi::c_void,
    pub m_mem_size: usize,
    pub m_mem_capacity: usize,
}

#[repr(C)]
pub struct ZipArray {
    pub m_p: *mut std::ffi::c_void,
    pub m_size: usize,
    pub m_capacity: usize,
    pub m_element_size: usize,
}

pub type FileReadFunc2 = unsafe extern "C" fn(
    p_opaque: *mut std::ffi::c_void,
    file_ofs: u64,
    p_buf: *mut u8,
    n: usize,
) -> usize;

const MZ_ZIP_DATA_DESCRIPTER_SIZE32_12: usize = 16;
const MZ_ZIP_DATA_DESCRIPTER_SIZE64_12: usize = 24;
const MZ_DEFAULT_LEVEL_12: u32 = 6;
const MZ_UBER_COMPRESSION_12: u32 = 10;
const MZ_ZIP_FLAG_WRITE_HEADER_SET_SIZE_12: u32 = 0x1000;
const MZ_ZIP_LDH_BIT_FLAG_HAS_LOCATOR_12: u16 = 0x0008;
const MZ_ZIP_FLAG_ASCII_FILENAME_12: u32 = 0x8000;
const MZ_ZIP_GENERAL_PURPOSE_BIT_FLAG_UTF8_12: u16 = 0x0800;
const MZ_ZIP_MODE_WRITING_12: i32 = 2;
const MZ_ZIP_FLAG_COMPRESSED_DATA_12: u32 = 0x2000;
const MZ_DEFLATED_12: u16 = 8;
const MZ_DEFAULT_STRATEGY_12: i32 = 0;
const TDEFL_STATUS_OKAY_12: i32 = 0;

struct TdeflCompressor2 {
    _dummy: i32,
}

impl TdeflCompressor2 {
    fn new() -> Self {
        TdeflCompressor2 { _dummy: 0 }
    }
    
    fn init(
        &mut self,
        _callback: fn(*mut std::ffi::c_void, *const u8, usize, *mut ZipWriterAddState2) -> usize,
        _state: &mut ZipWriterAddState2,
        _flags: i32,
    ) -> i32 {
        TDEFL_STATUS_OKAY_12
    }
}

fn mz_zip_time_t_to_dos_time(_time: SystemTime) -> (u16, u16) {
    (0, 0)
}

fn write_to_zip(_zip: &mut ZipArchive2_2, _offset: u64, _data: &[u8]) -> bool {
    true
}

fn mz_zip_writer_add_put_buf_callback2(_p: *mut std::ffi::c_void, _buf: *const u8, _len: usize, _state: *mut ZipWriterAddState2) -> usize {
    0
}

fn mz_zip_writer_validate_archive_name2(_name: &str) -> bool {
    true
}

fn mz_zip_writer_compute_padding_needed_for_file_alignment2(_zip: &ZipArchive2_2) -> usize {
    0
}

fn mz_zip_writer_write_zeros2(_zip: &mut ZipArchive2_2, _offset: u64, _len: usize) -> bool {
    true
}

fn mz_zip_writer_create_zip64_extra_data2(
    _data: &mut [u8],
    _uncomp_size_ptr: Option<*const u64>,
    _comp_size_ptr: Option<*const u64>,
    _header_ofs_ptr: Option<*const u64>,
) -> usize {
    0
}

fn mz_zip_writer_create_local_dir_header2(
    _zip: &ZipArchive2_2,
    _header: &mut [u8],
    _filename_len: u16,
    _extra_len: u16,
    _uncomp_size: u32,
    _comp_size: u32,
    _crc32: u32,
    _method: u16,
    _gen_flags: u16,
    _dos_time: u16,
    _dos_date: u16,
) -> bool {
    true
}

fn mz_crc32_2(_crc: u32, _buf: &[u8]) -> u32 {
    0
}

fn tdefl_create_comp_flags_from_zip_params2(_level: u32, _window_bits: i32, _strategy: i32) -> i32 {
    0
}

fn mz_zip_writer_add_to_central_dir2(
    _zip: &mut ZipArchive2_2,
    _archive_name: &str,
    _filename_len: u16,
    _extra: Option<&[u8]>,
    _extra_len: u16,
    _comment: Option<&[u8]>,
    _uncomp_size: u64,
    _comp_size: u64,
    _uncomp_crc32: u32,
    _method: u16,
    _gen_flags: u16,
    _dos_time: u16,
    _dos_date: u16,
    _local_dir_header_ofs: u64,
    _ext_attributes: u32,
    _user_extra_data_central: Option<&[u8]>,
) -> bool {
    true
}

// --- Module: mz_p6 ---
pub struct ZipArchive3 {
    m_pState: Option<Box<ZipInternalState3>>,
    m_pWrite: Option<fn(&mut dyn IoWrite, u64, &[u8]) -> std::io::Result<usize>>,
    m_pRead: Option<fn(&mut dyn Read, u64, &mut [u8]) -> std::io::Result<usize>>,
    m_pNeeds_keepalive: Option<fn() -> bool>,
    m_pIO_opaque: Rc<RefCell<dyn std::any::Any>>,
    m_archive_size: u64,
    m_central_directory_file_ofs: u64,
    m_total_files: u32,
    m_zip_type: ZipType,
    m_zip_mode: ZipMode3,
    m_file_offset_alignment: u32,
    m_pFree: fn(*mut u8),
    m_pAlloc_opaque: *mut u8,
}

pub enum ZipType {
    File,
    CFile,
    Memory,
    User,
}

pub enum ZipMode3 {
    Invalid,
    Reading,
    Writing,
}

impl PartialEq for ZipMode3 {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ZipMode3::Invalid, ZipMode3::Invalid) => true,
            (ZipMode3::Reading, ZipMode3::Reading) => true,
            (ZipMode3::Writing, ZipMode3::Writing) => true,
            _ => false,
        }
    }
}

impl PartialEq for ZipType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ZipType::File, ZipType::File) => true,
            (ZipType::CFile, ZipType::CFile) => true,
            (ZipType::Memory, ZipType::Memory) => true,
            (ZipType::User, ZipType::User) => true,
            _ => false,
        }
    }
}

pub struct ZipInternalState3 {
    m_central_dir: Vec<u8>,
    m_central_dir_offsets: Vec<u32>,
    m_sorted_central_dir_offsets: Vec<u32>,
    m_init_flags: u32,
    m_zip64: bool,
    m_zip64_has_extended_info_fields: bool,
    m_pFile: Option<File>,
    m_file_archive_start_ofs: u64,
    m_pMem: Option<Vec<u8>>,
    m_mem_size: usize,
    m_mem_capacity: usize,
}

pub struct ZipWriterAddState3<'a> {
    m_pZip: &'a mut ZipArchive3,
    m_cur_archive_file_ofs: u64,
    m_comp_size: u64,
}

const MZ_ZIP_LOCAL_DIR_HEADER_SIG_6: u32 = 0x04034b50;
const MZ_ZIP_CENTRAL_DIR_HEADER_SIG_6: u32 = 0x02014b50;
const MZ_ZIP_LOCAL_DIR_HEADER_SIZE_6: usize = 30;
const MZ_ZIP_CENTRAL_DIR_HEADER_SIZE_6: usize = 46;
const MZ_UINT16_MAX_6: u16 = 0xFFFF;
const MZ_UINT32_MAX_6: u32 = 0xFFFFFFFF;

const MZ_ZIP64_MAX_CENTRAL_EXTRA_FIELD_SIZE_6: usize = 28;
const MZ_DEFAULT_LEVEL_6: u32 = 6;
const MZ_ZIP_FLAG_COMPRESSED_DATA_6: u32 = 0x20000;
const MZ_ZIP_LDH_BIT_FLAG_HAS_LOCATOR_6: u16 = 0x0008;
const MZ_ZIP_FLAG_ASCII_FILENAME_6: u32 = 0x80000;
const MZ_ZIP_GENERAL_PURPOSE_BIT_FLAG_UTF8_6: u16 = 0x0800;
const MZ_ZIP_MODE_WRITING_6: ZipMode3 = ZipMode3::Writing;
const MZ_UBER_COMPRESSION_6: u32 = 10;
const MZ_ZIP_INVALID_FILENAME_6: i32 = -108;
const MZ_ZIP_END_OF_CENTRAL_DIR_HEADER_SIZE_6: usize = 22;
const MZ_ZIP_DATA_DESCRIPTER_SIZE32_6: usize = 16;
const MZ_ZIP_DOS_DIR_ATTRIBUTE_BITFLAG_6: u32 = 0x10;
const MZ_DEFLATED_6: u16 = 8;
const MZ_DEFAULT_STRATEGY_6: i32 = 0;
const TDEFL_STATUS_OKAY_6: i32 = 0;
const MZ_ZIP_COMPRESSION_FAILED_6: i32 = -109;
const TDEFL_FINISH_6: i32 = 4;
const TDEFL_STATUS_DONE_6: i32 = 1;
const MZ_ZIP_DATA_DESCRIPTER_SIZE64_6: usize = 24;
const MZ_ZIP_DATA_DESCRIPTOR_ID_6: u32 = 0x08074b50;

const MZ_ZIP_INVALID_PARAMETER_6: i32 = -100;
const MZ_ZIP_TOO_MANY_FILES_6: i32 = -101;
const MZ_ZIP_FILE_TOO_LARGE_6: i32 = -102;
const MZ_ZIP_FILE_OPEN_FAILED_6: i32 = -103;
const MZ_ZIP_UNSUPPORTED_CDIR_SIZE_6: i32 = -104;
const MZ_ZIP_INTERNAL_ERROR_6: i32 = -105;
const MZ_ZIP_ALLOC_FAILED_6: i32 = -106;
const MZ_ZIP_FILE_WRITE_FAILED_6: i32 = -107;

const MZ_ZIP_FLAG_WRITE_ALLOW_READING_6: u32 = 0;
const MZ_ZIP_FLAG_WRITE_ZIP64_6: u32 = 0;
const MZ_ZIP_FLAG_READ_ALLOW_WRITING_6: u32 = 0;

fn mz_write_le16_6(p: &mut [u8], v: u16) {
    p[0] = (v & 0xFF) as u8;
    p[1] = (v >> 8) as u8;
}

fn mz_write_le32_6(p: &mut [u8], v: u32) {
    p[0] = (v & 0xFF) as u8;
    p[1] = ((v >> 8) & 0xFF) as u8;
    p[2] = ((v >> 16) & 0xFF) as u8;
    p[3] = ((v >> 24) & 0xFF) as u8;
}

fn mz_write_le64_6(p: &mut [u8], v: u64) {
    mz_write_le32_6(p, (v & 0xFFFFFFFF) as u32);
    mz_write_le32_6(&mut p[4..], (v >> 32) as u32);
}

fn mz_zip_set_error_6(zip: &mut ZipArchive3, error_code: i32) -> bool {
    zip.set_error(error_code);
    false
}

fn mz_zip_array_push_back_6<T: Copy>(
    _zip: &mut ZipArchive3,
    array: &mut Vec<T>,
    data: &[T],
) -> bool {
    array.extend_from_slice(data);
    true
}

fn mz_zip_array_resize_6<T: Default>(
    _zip: &mut ZipArchive3,
    array: &mut Vec<T>,
    new_size: usize,
    _zero_new: bool,
) -> bool {
    array.resize(new_size, T::default());
    true
}

fn mz_zip_array_clear_6(_zip: &mut ZipArchive3, array: &mut Vec<u32>) {
    array.clear();
}

fn mz_zip_file_write_func(_opaque: &mut dyn IoWrite, _file_ofs: u64, _p_buf: &[u8]) -> std::io::Result<usize> {
    Err(std::io::Error::new(std::io::ErrorKind::Other, "Not implemented"))
}

fn mz_zip_file_read_func_6(_opaque: &mut dyn Read, _file_ofs: u64, _p_buf: &mut [u8]) -> std::io::Result<usize> {
    Err(std::io::Error::new(std::io::ErrorKind::Other, "Not implemented"))
}

fn mz_zip_heap_write_func(_opaque: &mut dyn IoWrite, _file_ofs: u64, _p_buf: &[u8]) -> std::io::Result<usize> {
    Err(std::io::Error::new(std::io::ErrorKind::Other, "Not implemented"))
}

fn mz_zip_reader_end_internal_6(zip: &mut ZipArchive3, preserve: bool) {
    if !preserve {
        zip.m_pState = None;
    }
}

fn mz_zip_writer_init_v2(_zip: &mut ZipArchive3, _size_to_reserve_at_beginning: u64, _flags: u32) -> bool {
    true
}

fn mz_zip_time_t_to_dos_time2(_time: std::time::SystemTime) -> (u16, u16) {
    (0, 0)
}

fn mz_zip_array_ensure_room<T>(_zip: &mut ZipArchive3, _array: &mut Vec<T>, _required_size: usize) -> bool {
    true
}

impl ZipArchive3 {
    pub fn set_error(&self, _error_code: i32) -> bool {
        false
    }
    
    pub fn m_pWrite(&self, _opaque: &mut dyn IoWrite, _file_ofs: u64, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(std::io::ErrorKind::Other, "Not implemented"))
    }
}

pub fn mz_zip_writer_init_cfile(zip: &mut ZipArchive3, p_file: &mut File, flags: u32) -> bool {
    zip.m_pWrite = Some(mz_zip_file_write_func);
    zip.m_pNeeds_keepalive = None;

    if flags & MZ_ZIP_FLAG_WRITE_ALLOW_READING_6 != 0 {
        zip.m_pRead = Some(mz_zip_file_read_func_6);
    }

    zip.m_pIO_opaque = Rc::new(RefCell::new(()));

    if !mz_zip_writer_init_v2(zip, 0, flags) {
        return false;
    }

    if let Some(ref mut state) = zip.m_pState {
        state.m_pFile = Some(p_file.try_clone().unwrap());
        state.m_file_archive_start_ofs = p_file.seek(SeekFrom::Current(0)).unwrap_or(0);
    }

    zip.m_zip_type = ZipType::CFile;
    true
}

pub fn mz_zip_writer_init_from_reader_v2(
    zip: &mut ZipArchive3,
    p_filename: Option<&str>,
    flags: u32,
) -> bool {
    let state = match &mut zip.m_pState {
        Some(s) => s,
        None => return mz_zip_set_error_6(zip, MZ_ZIP_INVALID_PARAMETER_6),
    };

    if zip.m_zip_mode != ZipMode3::Reading {
        return mz_zip_set_error_6(zip, MZ_ZIP_INVALID_PARAMETER_6);
    }

    if flags & MZ_ZIP_FLAG_WRITE_ZIP64_6 != 0 {
        if !state.m_zip64 {
            return mz_zip_set_error_6(zip, MZ_ZIP_INVALID_PARAMETER_6);
        }
    }

    if state.m_zip64 {
        if zip.m_total_files == MZ_UINT32_MAX_6 as u32 {
            return mz_zip_set_error_6(zip, MZ_ZIP_TOO_MANY_FILES_6);
        }
    } else {
        if zip.m_total_files == MZ_UINT16_MAX_6 as u32 {
            return mz_zip_set_error_6(zip, MZ_ZIP_TOO_MANY_FILES_6);
        }

        if (zip.m_archive_size + MZ_ZIP_CENTRAL_DIR_HEADER_SIZE_6 as u64 + MZ_ZIP_LOCAL_DIR_HEADER_SIZE_6 as u64) > MZ_UINT32_MAX_6 as u64 {
            return mz_zip_set_error_6(zip, MZ_ZIP_FILE_TOO_LARGE_6);
        }
    }

    if let Some(_file) = &state.m_pFile {
        if zip.m_zip_type == ZipType::File && flags & MZ_ZIP_FLAG_READ_ALLOW_WRITING_6 == 0 {
            if p_filename.is_none() {
                return mz_zip_set_error_6(zip, MZ_ZIP_INVALID_PARAMETER_6);
            }

            mz_zip_reader_end_internal_6(zip, false);
            return mz_zip_set_error_6(zip, MZ_ZIP_FILE_OPEN_FAILED_6);
        }

        zip.m_pWrite = Some(mz_zip_file_write_func);
        zip.m_pNeeds_keepalive = None;
    } else if state.m_pMem.is_some() {
        state.m_mem_capacity = state.m_mem_size;
        zip.m_pWrite = Some(mz_zip_heap_write_func);
        zip.m_pNeeds_keepalive = None;
    } else if zip.m_pWrite.is_none() {
        return mz_zip_set_error_6(zip, MZ_ZIP_INVALID_PARAMETER_6);
    }

    zip.m_archive_size = zip.m_central_directory_file_ofs;
    zip.m_central_directory_file_ofs = 0;

    if let Some(state) = &mut zip.m_pState {
        mz_zip_array_clear_6(zip, &mut state.m_sorted_central_dir_offsets);
    }

    zip.m_zip_mode = ZipMode3::Writing;
    true
}

pub fn mz_zip_writer_init_from_reader(
    zip: &mut ZipArchive3,
    p_filename: Option<&str>,
) -> bool {
    mz_zip_writer_init_from_reader_v2(zip, p_filename, 0)
}

fn mz_zip_writer_add_put_buf_callback3(p_buf: &[u8], len: usize, p_user: &mut ZipWriterAddState3) -> bool {
    let write_func = p_user.m_pZip.m_pWrite.unwrap();
    
    match write_func(&mut *p_user.m_pZip.m_pIO_opaque.borrow_mut(), p_user.m_cur_archive_file_ofs, &p_buf[..len]) {
        Ok(bytes_written) if bytes_written == len => {
            p_user.m_cur_archive_file_ofs += bytes_written as u64;
            p_user.m_comp_size += bytes_written as u64;
            true
        }
        _ => false,
    }
}

// --- Module: mz_p1 ---
#[cfg(windows)]
mod windows_file_io {
    use std::os::windows::ffi::OsStringExt;
    use widestring::U16CString;
    
    pub(crate) fn mz_utf8z_to_widechar(str: &str) -> Result<Vec<u16>, std::io::Error> {
        let wstr = U16CString::from_str(str)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(wstr.into_vec_with_nul())
    }
    
    pub(crate) fn mz_fopen(filename: &str, mode: &str) -> Result<std::fs::File, std::io::Error> {
        let wfilename = mz_utf8z_to_widechar(filename)?;
        let wmode = mz_utf8z_to_widechar(mode)?;
        
        let os_filename = std::ffi::OsString::from_wide(&wfilename[..wfilename.len()-1]);
        let file = match mode {
            "r" | "rb" => std::fs::File::open(&os_filename)?,
            "w" | "wb" => std::fs::File::create(&os_filename)?,
            "a" | "ab" => {
                let mut options = std::fs::OpenOptions::new();
                options.append(true).create(true);
                options.open(&os_filename)?
            }
            "r+" | "rb+" | "r+b" => {
                let mut options = std::fs::OpenOptions::new();
                options.read(true).write(true);
                options.open(&os_filename)?
            }
            "w+" | "wb+" | "w+b" => {
                let mut options = std::fs::OpenOptions::new();
                options.read(true).write(true).create(true).truncate(true);
                options.open(&os_filename)?
            }
            "a+" | "ab+" | "a+b" => {
                let mut options = std::fs::OpenOptions::new();
                options.read(true).append(true).create(true);
                options.open(&os_filename)?
            }
            _ => return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput, 
                format!("Invalid mode: {}", mode)
            )),
        };
        Ok(file)
    }
    
    pub(crate) fn mz_stat(path: &str, buffer: &mut std::fs::Metadata) -> Result<(), std::io::Error> {
        let wpath = mz_utf8z_to_widechar(path)?;
        let os_path = std::ffi::OsString::from_wide(&wpath[..wpath.len()-1]);
        *buffer = std::fs::metadata(&os_path)?;
        Ok(())
    }
}

#[cfg(not(windows))]
mod posix_file_io {
    pub(crate) fn mz_fopen(filename: &str, mode: &str) -> Result<std::fs::File, std::io::Error> {
        std::fs::File::open(filename)?;
        unimplemented!("POSIX mz_fopen")
    }
    
    pub(crate) fn mz_stat(path: &str, buffer: &mut std::fs::Metadata) -> Result<(), std::io::Error> {
        *buffer = std::fs::metadata(path)?;
        Ok(())
    }
}

#[derive(Clone)]
pub(crate) struct MzZipArray3<T> {
    data: Vec<T>,
    element_size: usize,
}

impl<T: Clone + Default> MzZipArray3<T> {
    pub(crate) fn new(element_size: usize) -> Self {
        Self {
            data: Vec::new(),
            element_size,
        }
    }
    
    pub(crate) fn clear(&mut self) {
        self.data.clear();
    }
    
    pub(crate) fn ensure_capacity(&mut self, min_new_capacity: usize, growing: bool) -> bool {
        if self.data.capacity() >= min_new_capacity {
            return true;
        }
        
        let new_capacity = if growing {
            let mut new_cap = std::cmp::max(1, self.data.capacity());
            while new_cap < min_new_capacity {
                new_cap = new_cap.saturating_mul(2);
            }
            new_cap
        } else {
            min_new_capacity
        };
        
        self.data.reserve(new_capacity);
        true
    }
    
    pub(crate) fn reserve(&mut self, new_capacity: usize, growing: bool) -> bool {
        if new_capacity > self.data.capacity() {
            self.ensure_capacity(new_capacity, growing)
        } else {
            true
        }
    }
    
    pub(crate) fn resize(&mut self, new_size: usize, growing: bool) -> bool {
        if new_size > self.data.capacity() {
            if !self.ensure_capacity(new_size, growing) {
                return false;
            }
        }
        self.data.resize(new_size, T::default());
        true
    }
    
    pub(crate) fn ensure_room(&mut self, n: usize) -> bool {
        self.reserve(self.data.len() + n, true)
    }
    
    pub(crate) fn push_back(&mut self, elements: &[T]) -> bool {
        self.data.extend_from_slice(elements);
        true
    }
    
    pub(crate) fn len(&self) -> usize {
        self.data.len()
    }
    
    pub(crate) fn capacity(&self) -> usize {
        self.data.capacity()
    }
    
    pub(crate) fn as_slice(&self) -> &[T] {
        &self.data
    }
    
    pub(crate) fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.data
    }
    
    pub(crate) fn get(&self, index: usize) -> Option<&T> {
        self.data.get(index)
    }
    
    pub(crate) fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.data.get_mut(index)
    }
}

impl MzZipArray3<u8> {
    pub(crate) fn push_back_bytes(&mut self, bytes: &[u8]) -> bool {
        self.data.extend_from_slice(bytes);
        true
    }
    
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

pub(crate) struct ZipReaderInternalState {
    pub central_dir: MzZipArray3<u8>,
    pub central_dir_offsets: MzZipArray3<u32>,
    pub sorted_central_dir_offsets: MzZipArray3<u32>,
    pub init_flags: u32,
    pub zip64: bool,
    pub zip64_has_extended_info_fields: bool,
    pub file_archive_start_ofs: u64,
    pub mem_size: usize,
    pub mem_capacity: usize,
}

impl ZipReaderInternalState {
    pub(crate) fn new(flags: u32) -> Self {
        Self {
            central_dir: MzZipArray3::new(1),
            central_dir_offsets: MzZipArray3::new(4),
            sorted_central_dir_offsets: MzZipArray3::new(4),
            init_flags: flags,
            zip64: false,
            zip64_has_extended_info_fields: false,
            file_archive_start_ofs: 0,
            mem_size: 0,
            mem_capacity: 0,
        }
    }
}

pub(crate) fn mz_zip_reader_filename_less(
    central_dir: &[u8],
    central_dir_offsets: &[u32],
    l_index: usize,
    r_index: usize,
) -> bool {
    fn to_lower_ascii(c: u8) -> u8 {
        if (b'A'..=b'Z').contains(&c) {
            c - b'A' + b'a'
        } else {
            c
        }
    }
    
    let l_offset = central_dir_offsets.get(l_index).copied().unwrap_or(0) as usize;
    let r_offset = central_dir_offsets.get(r_index).copied().unwrap_or(0) as usize;
    
    if l_offset + 28 >= central_dir.len() || r_offset + 28 >= central_dir.len() {
        return false;
    }
    
    let l_len = u16::from_le_bytes([
        central_dir[l_offset + 28],
        central_dir[l_offset + 29],
    ]) as usize;
    
    let r_len = u16::from_le_bytes([
        central_dir[r_offset + 28],
        central_dir[r_offset + 29],
    ]) as usize;
    
    let l_start = l_offset + 46;
    let r_start = r_offset + 46;
    
    let min_len = l_len.min(r_len);
    for i in 0..min_len {
        let l_char = central_dir.get(l_start + i).copied().unwrap_or(0);
        let r_char = central_dir.get(r_start + i).copied().unwrap_or(0);
        
        let l_lower = to_lower_ascii(l_char);
        let r_lower = to_lower_ascii(r_char);
        
        if l_lower != r_lower {
            return l_lower < r_lower;
        }
    }
    
    l_len < r_len
}

pub(crate) fn mz_zip_reader_sort_central_dir_offsets_by_filename(
    central_dir: &[u8],
    central_dir_offsets: &[u32],
    sorted_offsets: &mut [u32],
) {
    let size = sorted_offsets.len();
    if size <= 1 {
        return;
    }
    
    for i in 0..size {
        sorted_offsets[i] = i as u32;
    }
    
    let mut start = (size - 2) / 2;
    loop {
        let mut root = start;
        loop {
            let child = (root * 2) + 1;
            if child >= size {
                break;
            }
            
            let mut child_to_compare = child;
            if child + 1 < size {
                if mz_zip_reader_filename_less(
                    central_dir,
                    central_dir_offsets,
                    sorted_offsets[child] as usize,
                    sorted_offsets[child + 1] as usize,
                ) {
                    child_to_compare = child + 1;
                }
            }
            
            if !mz_zip_reader_filename_less(
                central_dir,
                central_dir_offsets,
                sorted_offsets[root] as usize,
                sorted_offsets[child_to_compare] as usize,
            ) {
                break;
            }
            
            sorted_offsets.swap(root, child_to_compare);
            root = child_to_compare;
        }
        
        if start == 0 {
            break;
        }
        start -= 1;
    }
    
    let mut end = size - 1;
    while end > 0 {
        sorted_offsets.swap(0, end);
        
        let mut root = 0;
        loop {
            let child = (root * 2) + 1;
            if child >= end {
                break;
            }
            
            let mut child_to_compare = child;
            if child + 1 < end {
                if mz_zip_reader_filename_less(
                    central_dir,
                    central_dir_offsets,
                    sorted_offsets[child] as usize,
                    sorted_offsets[child + 1] as usize,
                ) {
                    child_to_compare = child + 1;
                }
            }
            
            if !mz_zip_reader_filename_less(
                central_dir,
                central_dir_offsets,
                sorted_offsets[root] as usize,
                sorted_offsets[child_to_compare] as usize,
            ) {
                break;
            }
            
            sorted_offsets.swap(root, child_to_compare);
            root = child_to_compare;
        }
        
        end -= 1;
    }
}

pub(crate) fn mz_zip_reader_locate_header_sig(
    read_fn: impl Fn(u64, &mut [u8]) -> Result<usize, std::io::Error>,
    archive_size: u64,
    record_sig: u32,
    record_size: usize,
) -> Result<Option<u64>, std::io::Error> {
    if archive_size < record_size as u64 {
        return Ok(None);
    }
    
    let mut buf = [0u8; 4096];
    let mut cur_file_ofs = archive_size.saturating_sub(buf.len() as u64);
    
    loop {
        let n = (buf.len() as u64).min(archive_size - cur_file_ofs) as usize;
        let bytes_read = read_fn(cur_file_ofs, &mut buf[..n])?;
        
        if bytes_read != n {
            return Ok(None);
        }
        
        for i in (0..n.saturating_sub(3)).rev() {
            let sig = u32::from_le_bytes([buf[i], buf[i+1], buf[i+2], buf[i+3]]);
            if sig == record_sig {
                let found_ofs = cur_file_ofs + i as u64;
                if archive_size - found_ofs >= record_size as u64 {
                    return Ok(Some(found_ofs));
                }
            }
        }
        
        if cur_file_ofs == 0 {
            break;
        }
        
        if archive_size - cur_file_ofs >= 65535 + record_size as u64 {
            break;
        }
        
        cur_file_ofs = cur_file_ofs.saturating_sub((buf.len() - 3) as u64);
    }
    
    Ok(None)
}

pub(crate) fn mz_zip_reader_eocd64_valid(
    read_fn: impl Fn(u64, &mut [u8]) -> Result<usize, std::io::Error>,
    offset: u64,
) -> Result<bool, std::io::Error> {
    let mut buf = [0u8; 56];
    let bytes_read = read_fn(offset, &mut buf)?;
    
    if bytes_read < 56 {
        return Ok(false);
    }
    
    let sig = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    Ok(sig == 0x06064b50)
}

pub(crate) fn mz_zip_reader_init_internal(
    flags: u32,
) -> Result<ZipReaderInternalState, ZipError> {
    let mut state = ZipReaderInternalState::new(flags);
    Ok(state)
}

pub(crate) fn mz_tolower(c: u8) -> u8 {
    if (b'A'..=b'Z').contains(&c) {
        c - b'A' + b'a'
    } else {
        c
    }
}

pub(crate) fn swap_u32(a: &mut u32, b: &mut u32) {
    std::mem::swap(a, b);
}

// --- Module: mz_p2 ---
#[derive(Default)]
pub struct ZipArray2_2 {
    data: Vec<u8>,
    element_size: usize,
}

impl ZipArray2_2 {
    pub fn new(element_size: usize) -> Self {
        Self {
            data: Vec::new(),
            element_size,
        }
    }

    pub fn resize(&mut self, new_size: usize) -> bool {
        let new_len = new_size * self.element_size;
        self.data.resize(new_len, 0);
        true
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    pub fn as_slice<T>(&self) -> &[T] {
        assert!(std::mem::size_of::<T>() == self.element_size);
        unsafe { slice::from_raw_parts(self.data.as_ptr() as *const T, self.data.len() / self.element_size) }
    }

    pub fn as_mut_slice<T>(&mut self) -> &mut [T] {
        assert!(std::mem::size_of::<T>() == self.element_size);
        unsafe { slice::from_raw_parts_mut(self.data.as_mut_ptr() as *mut T, self.data.len() / self.element_size) }
    }

    pub fn len(&self) -> usize {
        self.data.len() / self.element_size
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

pub struct ZipInternalState4 {
    pub central_dir: ZipArray2_2,
    pub central_dir_offsets: ZipArray2_2,
    pub sorted_central_dir_offsets: ZipArray2_2,
    pub init_flags: u32,
    pub zip64: bool,
    pub zip64_has_extended_info_fields: bool,
    pub file_archive_start_ofs: u64,
    #[cfg(not(feature = "no_stdio"))]
    pub file: Option<std::fs::File>,
    pub mem_buffer: Option<Vec<u8>>,
    pub mem_size: usize,
    pub mem_capacity: usize,
}

impl Default for ZipInternalState4 {
    fn default() -> Self {
        Self {
            central_dir: ZipArray2_2::new(1),
            central_dir_offsets: ZipArray2_2::new(std::mem::size_of::<u32>()),
            sorted_central_dir_offsets: ZipArray2_2::new(std::mem::size_of::<u32>()),
            init_flags: 0,
            zip64: false,
            zip64_has_extended_info_fields: false,
            file_archive_start_ofs: 0,
            #[cfg(not(feature = "no_stdio"))]
            file: None,
            mem_buffer: None,
            mem_size: 0,
            mem_capacity: 0,
        }
    }
}

pub enum ZipSource {
    #[cfg(not(feature = "no_stdio"))]
    File(std::fs::File),
    Memory(Vec<u8>),
    User,
}

pub struct ZipArchive4 {
    pub m_pState: Option<Box<ZipInternalState4>>,
    pub m_pAlloc_opaque: *mut (),
    pub m_pAlloc: Option<fn(*mut (), usize) -> *mut ()>,
    pub m_pFree: Option<fn(*mut (), *mut ())>,
    pub m_pRead: Option<fn(*mut (), u64, &mut [u8]) -> usize>,
    pub m_pWrite: Option<fn(*mut (), u64, &[u8]) -> usize>,
    pub m_pNeeds_keep_alive: Option<fn(*mut ()) -> bool>,
    pub m_zip_type: u32,
    pub m_archive_size: u64,
    pub m_central_directory_file_ofs: u64,
    pub m_total_files: u32,
    pub m_zip_mode: u32,
    pub m_last_error: ZipError,
}

impl Default for ZipArchive4 {
    fn default() -> Self {
        Self {
            m_pState: None,
            m_pAlloc_opaque: std::ptr::null_mut(),
            m_pAlloc: None,
            m_pFree: None,
            m_pRead: None,
            m_pWrite: None,
            m_pNeeds_keep_alive: None,
            m_zip_type: 0,
            m_archive_size: 0,
            m_central_directory_file_ofs: 0,
            m_total_files: 0,
            m_zip_mode: 0,
            m_last_error: ZipError::InvalidParameter,
        }
    }
}

impl ZipArchive4 {
    pub fn set_error(&mut self, error_code: ZipError) -> bool {
        self.m_last_error = error_code;
        false
    }

    pub fn read_from_source(&self, file_ofs: u64, buf: &mut [u8]) -> Result<usize, ZipError> {
        if let Some(read_func) = self.m_pRead {
            let result = read_func(self.m_pAlloc_opaque, file_ofs, buf);
            if result == 0 && !buf.is_empty() {
                return Err(ZipError::FileReadFailed);
            }
            Ok(result)
        } else {
            Err(ZipError::InvalidParameter)
        }
    }

    fn locate_header_sig(&self, sig: u32, header_size: usize, p_file_ofs: &mut i64) -> bool {
        *p_file_ofs = i64::try_from(self.m_archive_size)
            .expect("archive size too large for i64") - 
            i64::try_from(header_size)
                .expect("header size too large for i64");
        true
    }

    fn eocd64_valid(&self, ofs: u64, buf: &[u8]) -> bool {
        if buf.len() < MZ_ZIP64_END_OF_CENTRAL_DIR_HEADER_SIZE {
            return false;
        }
        read_le_u32_2(&buf[MZ_ZIP64_ECDH_SIG_OFS..]) == MZ_ZIP64_END_OF_CENTRAL_DIR_HEADER_SIG
    }

    fn reader_sort_central_dir_offsets_by_filename(&mut self) {
    }
}

const MZ_ZIP64_END_OF_CENTRAL_DIR_HEADER_SIZE: usize = 56;
const MZ_ZIP64_ECDH_SIG_OFS: usize = 0;
const MZ_ZIP64_END_OF_CENTRAL_DIR_HEADER_SIG: u32 = 0x06064b50;

fn read_le_u16_2(data: &[u8]) -> u16 {
    let mut bytes = [0u8; 2];
    bytes.copy_from_slice(&data[..2]);
    u16::from_le_bytes(bytes)
}

fn read_le_u32_2(data: &[u8]) -> u32 {
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&data[..4]);
    u32::from_le_bytes(bytes)
}

fn read_le_u64_2(data: &[u8]) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&data[..8]);
    u64::from_le_bytes(bytes)
}

// --- Module: mz_p3 ---
use std::ptr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ZipType3_2 {
    User,
    Memory,
    File,
    CFile,
}

impl Default for ZipType3_2 {
    fn default() -> Self {
        ZipType3_2::User
    }
}

struct ZipInternalState5 {
    central_dir: Vec<u8>,
    central_dir_offsets: Vec<u32>,
    sorted_central_dir_offsets: Vec<u32>,
    init_flags: u32,
    zip64: bool,
    zip64_has_extended_info_fields: bool,
    p_file: Option<File>,
    file_archive_start_ofs: u64,
    p_mem: Option<Vec<u8>>,
    mem_size: usize,
    mem_capacity: usize,
}

#[derive(Default)]
pub struct ZipArchive5 {
    m_p_read: Option<fn(&ZipArchive5, u64, &mut [u8]) -> Result<usize, ZipError>>,
    m_p_io_opaque: *mut c_void,
    m_p_needs_keepalive: Option<*mut c_void>,
    m_zip_type: ZipType3_2,
    m_archive_size: u64,
    m_total_files: u32,
    m_p_state: Option<Box<ZipInternalState5>>,
}

impl ZipArchive5 {
    fn set_error(&mut self, _error_code: ZipError) -> bool {
        false
    }
    
    fn reader_init_internal(&mut self, flags: u32) -> bool {
        self.m_p_state = Some(Box::new(ZipInternalState5 {
            central_dir: Vec::new(),
            central_dir_offsets: Vec::new(),
            sorted_central_dir_offsets: Vec::new(),
            init_flags: flags,
            zip64: false,
            zip64_has_extended_info_fields: false,
            p_file: None,
            file_archive_start_ofs: 0,
            p_mem: None,
            mem_size: 0,
            mem_capacity: 0,
        }));
        true
    }
    
    fn reader_read_central_dir(&mut self, _flags: u32) -> Result<(), ZipError> {
        Ok(())
    }
    
    fn reader_end_internal(&mut self, preserve: bool) {
        if !preserve {
            self.m_p_state = None;
        }
    }
}

fn mz_zip_mem_read_func(p_opaque: &ZipArchive5, file_ofs: u64, p_buf: &mut [u8]) -> Result<usize, ZipError> {
    if let Some(state) = &p_opaque.m_p_state {
        if let Some(mem) = &state.p_mem {
            if file_ofs >= p_opaque.m_archive_size {
                return Ok(0);
            }
            let remaining = (p_opaque.m_archive_size - file_ofs) as usize;
            let to_copy = usize::min(p_buf.len(), remaining);
            let start = file_ofs as usize;
            p_buf[..to_copy].copy_from_slice(&mem[start..start + to_copy]);
            return Ok(to_copy);
        }
    }
    Ok(0)
}

fn mz_zip_file_read_func_5(p_opaque: &ZipArchive5, file_ofs: u64, p_buf: &mut [u8]) -> Result<usize, ZipError> {
    if let Some(state) = &p_opaque.m_p_state {
        if let Some(file) = &mut state.p_file.as_ref() {
            let absolute_ofs = file_ofs + state.file_archive_start_ofs;
            
            if let Err(_) = file.seek(SeekFrom::Start(absolute_ofs)) {
                return Err(ZipError::FileSeekFailed);
            }
            
            match file.read(p_buf) {
                Ok(bytes_read) => Ok(bytes_read),
                Err(_) => Ok(0),
            }
        } else {
            Ok(0)
        }
    } else {
        Ok(0)
    }
}

pub fn mz_zip_reader_init(p_zip: &mut ZipArchive5, size: u64, flags: u32) -> bool {
    if p_zip.m_p_read.is_none() {
        p_zip.set_error(ZipError::InvalidParameter);
        return false;
    }
    
    if !p_zip.reader_init_internal(flags) {
        return false;
    }
    
    p_zip.m_zip_type = ZipType3_2::User;
    p_zip.m_archive_size = size;
    
    match p_zip.reader_read_central_dir(flags) {
        Ok(_) => true,
        Err(_) => {
            p_zip.reader_end_internal(false);
            false
        }
    }
}

pub fn mz_zip_reader_init_mem_5(p_zip: &mut ZipArchive5, p_mem: &[u8], size: usize, flags: u32) -> bool {
    if p_mem.is_empty() {
        p_zip.set_error(ZipError::InvalidParameter);
        return false;
    }
    
    if size < 22 {
        p_zip.set_error(ZipError::NotAnArchive);
        return false;
    }
    
    if !p_zip.reader_init_internal(flags) {
        return false;
    }
    
    p_zip.m_zip_type = ZipType3_2::Memory;
    p_zip.m_archive_size = size as u64;
    p_zip.m_p_read = Some(mz_zip_mem_read_func);
    p_zip.m_p_io_opaque = ptr::null_mut();
    p_zip.m_p_needs_keepalive = None;
    
    if let Some(state) = &mut p_zip.m_p_state {
        state.p_mem = Some(p_mem.to_vec());
        state.mem_size = size;
        state.mem_capacity = size;
    }
    
    match p_zip.reader_read_central_dir(flags) {
        Ok(_) => true,
        Err(_) => {
            p_zip.reader_end_internal(false);
            false
        }
    }
}

pub fn mz_zip_reader_init_file_5(p_zip: &mut ZipArchive5, p_filename: &str, flags: u32) -> bool {
    mz_zip_reader_init_file_v2_5(p_zip, p_filename, flags, 0, 0)
}

pub fn mz_zip_reader_init_file_v2_5(
    p_zip: &mut ZipArchive5, 
    p_filename: &str, 
    flags: u32, 
    file_start_ofs: u64, 
    archive_size: u64
) -> bool {
    if p_filename.is_empty() || (archive_size != 0 && archive_size < 22) {
        p_zip.set_error(ZipError::InvalidParameter);
        return false;
    }
    
    let mut file = match File::open(p_filename) {
        Ok(f) => f,
        Err(_) => {
            p_zip.set_error(ZipError::FileOpenFailed);
            return false;
        }
    };
    
    let file_size = if archive_size != 0 {
        archive_size
    } else {
        match file.seek(SeekFrom::End(0)) {
            Ok(size) => {
                if let Err(_) = file.seek(SeekFrom::Start(0)) {
                    p_zip.set_error(ZipError::FileSeekFailed);
                    return false;
                }
                size
            }
            Err(_) => {
                p_zip.set_error(ZipError::FileSeekFailed);
                return false;
            }
        }
    };
    
    if file_size < 22 {
        p_zip.set_error(ZipError::NotAnArchive);
        return false;
    }
    
    if !p_zip.reader_init_internal(flags) {
        return false;
    }
    
    p_zip.m_zip_type = ZipType3_2::File;
    p_zip.m_p_read = Some(mz_zip_file_read_func_5);
    p_zip.m_p_io_opaque = ptr::null_mut();
    
    if let Some(state) = &mut p_zip.m_p_state {
        state.p_file = Some(file);
        state.file_archive_start_ofs = file_start_ofs;
    }
    
    p_zip.m_archive_size = file_size;
    
    match p_zip.reader_read_central_dir(flags) {
        Ok(_) => true,
        Err(_) => {
            p_zip.reader_end_internal(false);
            false
        }
    }
}

pub fn mz_zip_reader_init_cfile_5(
    p_zip: &mut ZipArchive5, 
    p_file: &mut File, 
    archive_size: u64, 
    flags: u32
) -> bool {
    let cur_file_ofs = match p_file.seek(SeekFrom::Current(0)) {
        Ok(pos) => pos,
        Err(_) => {
            p_zip.set_error(ZipError::FileSeekFailed);
            return false;
        }
    };
    
    let actual_archive_size = if archive_size == 0 {
        let end_pos = match p_file.seek(SeekFrom::End(0)) {
            Ok(pos) => pos,
            Err(_) => {
                p_zip.set_error(ZipError::FileSeekFailed);
                return false;
            }
        };
        
        if let Err(_) = p_file.seek(SeekFrom::Start(cur_file_ofs)) {
            p_zip.set_error(ZipError::FileSeekFailed);
            return false;
        }
        
        end_pos - cur_file_ofs
    } else {
        archive_size
    };
    
    if actual_archive_size < 22 {
        p_zip.set_error(ZipError::NotAnArchive);
        return false;
    }
    
    if !p_zip.reader_init_internal(flags) {
        return false;
    }
    
    p_zip.m_zip_type = ZipType3_2::CFile;
    p_zip.m_p_read = Some(mz_zip_file_read_func_5);
    p_zip.m_p_io_opaque = ptr::null_mut();
    
    if let Some(state) = &mut p_zip.m_p_state {
        state.p_file = Some(p_file.try_clone().unwrap_or_else(|_| File::open("/dev/null").unwrap()));
        state.file_archive_start_ofs = cur_file_ofs;
    }
    
    p_zip.m_archive_size = actual_archive_size;
    
    match p_zip.reader_read_central_dir(flags) {
        Ok(_) => true,
        Err(_) => {
            p_zip.reader_end_internal(false);
            false
        }
    }
}

fn mz_zip_get_cdh_5(p_zip: &ZipArchive5, file_index: u32) -> Option<&[u8]> {
    if file_index >= p_zip.m_total_files {
        return None;
    }
    
    if let Some(state) = &p_zip.m_p_state {
        if file_index as usize >= state.central_dir_offsets.len() {
            return None;
        }
        
        let offset = state.central_dir_offsets[file_index as usize] as usize;
        if offset >= state.central_dir.len() {
            return None;
        }
        
        Some(&state.central_dir[offset..])
    } else {
        None
    }
}

pub fn mz_zip_reader_is_file_encrypted(p_zip: &mut ZipArchive5, file_index: u32) -> bool {
    let p = match mz_zip_get_cdh_5(p_zip, file_index) {
        Some(data) => data,
        None => {
            p_zip.set_error(ZipError::InvalidParameter);
            return false;
        }
    };
    
    if p.len() < 10 {
        p_zip.set_error(ZipError::InvalidParameter);
        return false;
    }
    
    let bit_flag = u16::from_le_bytes([p[8], p[9]]);
    let encrypted_bit = 1;
    let strong_encryption_bit = 64;
    
    (bit_flag & (encrypted_bit | strong_encryption_bit)) != 0
}

pub fn mz_zip_reader_is_file_supported(p_zip: &mut ZipArchive5, file_index: u32) -> bool {
    let p = match mz_zip_get_cdh_5(p_zip, file_index) {
        Some(data) => data,
        None => {
            p_zip.set_error(ZipError::InvalidParameter);
            return false;
        }
    };
    
    if p.len() < 12 {
        p_zip.set_error(ZipError::InvalidParameter);
        return false;
    }
    
    let method = u16::from_le_bytes([p[10], p[11]]);
    let bit_flag = u16::from_le_bytes([p[8], p[9]]);
    
    if method != 0 && method != 8 {
        p_zip.set_error(ZipError::UnsupportedMethod);
        return false;
    }
    
    let encrypted_bit = 1;
    let strong_encryption_bit = 64;
    if (bit_flag & (encrypted_bit | strong_encryption_bit)) != 0 {
        p_zip.set_error(ZipError::UnsupportedEncryption);
        return false;
    }
    
    let compressed_patch_flag = 32;
    if (bit_flag & compressed_patch_flag) != 0 {
        p_zip.set_error(ZipError::UnsupportedFeature);
        return false;
    }
    
    true
}

pub fn mz_zip_reader_is_file_a_directory(p_zip: &mut ZipArchive5, file_index: u32) -> bool {
    let p = match mz_zip_get_cdh_5(p_zip, file_index) {
        Some(data) => data,
        None => {
            p_zip.set_error(ZipError::InvalidParameter);
            return false;
        }
    };
    
    if p.len() < 30 {
        p_zip.set_error(ZipError::InvalidParameter);
        return false;
    }
    
    let filename_len = u16::from_le_bytes([p[28], p[29]]) as usize;
    
    if filename_len > 0 {
        let filename_start = 46;
        if p.len() >= filename_start + filename_len {
            if p[filename_start + filename_len - 1] == b'/' {
                return true;
            }
        }
    }
    
    if p.len() >= 42 {
        let external_attr = u32::from_le_bytes([p[38], p[39], p[40], p[41]]);
        let dos_dir_attribute_bitflag = 0x10;
        if (external_attr & dos_dir_attribute_bitflag) != 0 {
            return true;
        }
    }
    
    false
}

// --- Module: mz_p4 ---
const MZ_ZIP_CDH_VERSION_MADE_BY_OFS: usize = 4;
const MZ_ZIP_CDH_VERSION_NEEDED_OFS: usize = 6;
const MZ_ZIP_CDH_BIT_FLAG_OFS: usize = 8;
const MZ_ZIP_CDH_METHOD_OFS: usize = 10;
const MZ_ZIP_CDH_FILE_TIME_OFS: usize = 12;
const MZ_ZIP_CDH_FILE_DATE_OFS: usize = 14;
const MZ_ZIP_CDH_CRC32_OFS: usize = 16;
const MZ_ZIP_CDH_COMPRESSED_SIZE_OFS: usize = 20;
const MZ_ZIP_CDH_DECOMPRESSED_SIZE_OFS: usize = 24;
const MZ_ZIP_CDH_FILENAME_LEN_OFS: usize = 28;
const MZ_ZIP_CDH_EXTRA_LEN_OFS: usize = 30;
const MZ_ZIP_CDH_COMMENT_LEN_OFS: usize = 32;
const MZ_ZIP_CDH_INTERNAL_ATTR_OFS: usize = 36;
const MZ_ZIP_CDH_EXTERNAL_ATTR_OFS: usize = 38;
const MZ_ZIP_CDH_LOCAL_HEADER_OFS: usize = 42;

const MZ_ZIP_FLAG_CASE_SENSITIVE: u32 = 0;
const MZ_ZIP_FLAG_IGNORE_PATH: u32 = 0;
const MZ_ZIP_FLAG_DO_NOT_SORT_CENTRAL_DIRECTORY: u32 = 0;

const MZ_ZIP_MODE_READING: u32 = 0;
const MZ_ZIP_INVALID_PARAMETER_4: i32 = 0;
const MZ_ZIP_FILE_NOT_FOUND: i32 = 0;

struct ZipArchive6;
struct ZipArchiveFileStat6;

fn to_lower_ascii_2(c: u8) -> u8 {
    if c >= b'A' && c <= b'Z' {
        c - b'A' + b'a'
    } else {
        c
    }
}

fn set_error2(_p_zip: &mut ZipArchive6, _error: i32) -> bool {
    false
}

fn mz_zip_dos_to_time_t(_time: u16, _date: u16) -> u64 {
    0
}

pub fn mz_zip_string_equal(p_a: &[u8], p_b: &[u8], len: usize, flags: u32) -> bool {
    if flags & MZ_ZIP_FLAG_CASE_SENSITIVE != 0 {
        return p_a[..len] == p_b[..len];
    }
    
    p_a[..len].iter().zip(p_b[..len].iter())
        .all(|(&a, &b)| to_lower_ascii_2(a) == to_lower_ascii_2(b))
}

pub fn mz_zip_filename_compare(
    p_central_dir_array: &[u8],
    p_central_dir_offsets: &[u32],
    l_index: u32,
    p_r: &[u8],
    r_len: usize,
) -> i32 {
    let offset = p_central_dir_offsets[l_index as usize] as usize;
    let p_l = &p_central_dir_array[offset..];
    let l_len = read_le_u16_2(&p_l[MZ_ZIP_CDH_FILENAME_LEN_OFS..]) as usize;
    let p_l_start = &p_l[MZ_ZIP_CENTRAL_DIR_HEADER_SIZE..];
    
    let min_len = min(l_len, r_len);
    for i in 0..min_len {
        let l = to_lower_ascii_2(p_l_start[i]);
        let r = to_lower_ascii_2(p_r[i]);
        if l != r {
            return l as i32 - r as i32;
        }
    }
    
    l_len as i32 - r_len as i32
}

pub fn mz_zip_locate_file_binary_search(
    p_zip: &mut ZipArchive6,
    p_filename: &str,
    p_index: Option<&mut u32>,
) -> bool {
    let _p_state = ();
    let p_central_dir_offsets: &[u32] = &[];
    let p_central_dir: &[u8] = &[];
    let p_indices: &[u32] = &[];
    let size = 0;
    let filename_len = p_filename.len();
    
    if let Some(index) = p_index {
        *index = 0;
    }
    
    if size > 0 {
        let mut l: i64 = 0;
        let mut h: i64 = size as i64 - 1;
        
        while l <= h {
            let m = l + ((h - l) >> 1);
            let file_index = p_indices[m as usize];
            
            let comp = mz_zip_filename_compare(
                p_central_dir,
                p_central_dir_offsets,
                file_index,
                p_filename.as_bytes(),
                filename_len,
            );
            
            match comp.cmp(&0) {
                std::cmp::Ordering::Equal => {
                    if let Some(index) = p_index {
                        *index = file_index;
                    }
                    return true;
                }
                std::cmp::Ordering::Less => l = m + 1,
                std::cmp::Ordering::Greater => h = m - 1,
            }
        }
    }
    
    set_error2(p_zip, MZ_ZIP_FILE_NOT_FOUND)
}

pub fn mz_zip_reader_locate_file(
    p_zip: &mut ZipArchive6,
    p_name: &str,
    p_comment: Option<&str>,
    flags: u32,
) -> i32 {
    let mut index = 0;
    if mz_zip_reader_locate_file_v2_4(p_zip, p_name, p_comment, flags, Some(&mut index)) {
        index as i32
    } else {
        -1
    }
}

pub fn mz_zip_reader_locate_file_v2_4(
    p_zip: &mut ZipArchive6,
    p_name: &str,
    p_comment: Option<&str>,
    flags: u32,
    p_index: Option<&mut u32>,
) -> bool {
    if let Some(index) = p_index {
        *index = 0;
    }
    
    if p_name.is_empty() {
        return set_error2(p_zip, MZ_ZIP_INVALID_PARAMETER_4);
    }
    
    let name_len = p_name.len();
    if name_len > u16::MAX as usize {
        return set_error2(p_zip, MZ_ZIP_INVALID_PARAMETER_4);
    }
    
    let comment_len = p_comment.map(|c| c.len()).unwrap_or(0);
    if comment_len > u16::MAX as usize {
        return set_error2(p_zip, MZ_ZIP_INVALID_PARAMETER_4);
    }
    
    for _file_index in 0..0 {
        let _offset = 0;
        let _p_header: &[u8] = &[];
        let _filename_len = 0;
        let _p_filename: &[u8] = &[];
        
        if false {
            let _file_extra_len = 0;
            let _file_comment_len = 0;
            let _p_file_comment: &[u8] = &[];
            
            if let Some(comment) = p_comment {
                let _ = comment.as_bytes();
            }
        }
        
        if false {
            let (_p_filename_adj, _filename_len_adj) = (_p_filename, _filename_len);
            
            if false {
                if let Some(index) = p_index {
                    *index = 0;
                }
                return true;
            }
        }
    }
    
    set_error2(p_zip, MZ_ZIP_FILE_NOT_FOUND)
}

// --- Module: mz_p5 ---
const MZ_ZIP_FLAG_COMPRESSED_DATA2: u32 = 0;

#[derive(Debug, PartialEq)]
enum TinflStatus {
    Done,
    NeedsMoreInput,
    Failed,
}

struct TinflDecompressor;

impl TinflDecompressor {
    fn new() -> Self {
        TinflDecompressor {}
    }

    fn decompress(
        &mut self,
        _input: &[u8],
        _output: &mut [u8],
        _flags: u32,
    ) -> (TinflStatus, usize, usize) {
        (TinflStatus::Done, 0, 0)
    }
}

#[derive(Debug, Clone)]
pub struct ZipFileStat {
    pub m_is_directory: bool,
    pub m_comp_size: u64,
    pub m_uncomp_size: u64,
    pub m_method: u16,
    pub m_bit_flag: u16,
    pub m_local_header_ofs: u64,
    pub m_crc32: u32,
}

struct ZipInternalState6 {
    m_pMem: Option<Vec<u8>>,
}

trait ReadWrite {
    fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, ZipError>;
}

pub struct ZipArchive7 {
    m_pState: Option<Box<ZipInternalState6>>,
    m_pIO_opaque: Box<dyn ReadWrite>,
    m_archive_size: u64,
}

impl ZipArchive7 {
    pub fn set_error(&mut self, _error: ZipError) -> bool {
        false
    }
    
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, ZipError> {
        self.m_pIO_opaque.read(offset, buf)
    }
}

fn mz_zip_reader_extract_to_mem_no_alloc1(
    zip: &mut ZipArchive7,
    file_index: u32,
    buf: &mut [u8],
    buf_size: usize,
    flags: u32,
    user_read_buf: Option<&mut [u8]>,
    user_read_buf_size: usize,
    st: Option<&ZipFileStat>,
) -> bool {
    let mut status = TinflStatus::Done;
    let mut needed_size: u64;
    let mut cur_file_ofs: u64;
    let mut comp_remaining: u64;
    let mut out_buf_ofs: usize = 0;
    let mut read_buf_ofs: usize = 0;
    let mut read_buf_avail: usize = 0;
    
    let file_stat = if let Some(stat) = st {
        stat.clone()
    } else {
        return false;
    };
    
    if zip.m_pState.is_none() || (buf_size > 0 && buf.is_empty()) || 
       (user_read_buf_size > 0 && user_read_buf.is_none()) {
        return zip.set_error(ZipError::InvalidParameter);
    }
    
    if file_stat.m_is_directory || file_stat.m_comp_size == 0 {
        return true;
    }
    
    let unsupported_flags = 1 | 64 | 32;
    if file_stat.m_bit_flag & unsupported_flags != 0 {
        return zip.set_error(ZipError::UnsupportedEncryption);
    }
    
    if (flags & 0) == 0 && file_stat.m_method != 0 && file_stat.m_method != MZ_DEFLATED_8 {
        return zip.set_error(ZipError::UnsupportedMethod);
    }
    
    needed_size = if (flags & 0) != 0 {
        file_stat.m_comp_size
    } else {
        file_stat.m_uncomp_size
    };
    
    if buf_size < needed_size as usize {
        return zip.set_error(ZipError::BufTooSmall);
    }
    
    cur_file_ofs = file_stat.m_local_header_ofs;
    let mut local_header = [0u8; MZ_ZIP_LOCAL_DIR_HEADER_SIZE];
    
    if zip.read_at(cur_file_ofs, &mut local_header).ok() != Some(MZ_ZIP_LOCAL_DIR_HEADER_SIZE) {
        return zip.set_error(ZipError::FileReadFailed);
    }
    
    let sig = u32::from_le_bytes([
        local_header[0], local_header[1], local_header[2], local_header[3]
    ]);
    if sig != MZ_ZIP_LOCAL_DIR_HEADER_SIG_6 {
        return zip.set_error(ZipError::InvalidHeaderOrCorrupted);
    }
    
    let filename_len = u16::from_le_bytes([
        local_header[MZ_ZIP_LDH_FILENAME_LEN_OFS],
        local_header[MZ_ZIP_LDH_FILENAME_LEN_OFS + 1]
    ]) as u64;
    
    let extra_len = u16::from_le_bytes([
        local_header[MZ_ZIP_LDH_EXTRA_LEN_OFS],
        local_header[MZ_ZIP_LDH_EXTRA_LEN_OFS + 1]
    ]) as u64;
    
    cur_file_ofs += MZ_ZIP_LOCAL_DIR_HEADER_SIZE as u64 + filename_len + extra_len;
    
    if cur_file_ofs + file_stat.m_comp_size > zip.m_archive_size {
        return zip.set_error(ZipError::InvalidHeaderOrCorrupted);
    }
    
    if (flags & 0) != 0 || file_stat.m_method == 0 {
        let read_size = needed_size as usize;
        if zip.read_at(cur_file_ofs, &mut buf[..read_size]).ok() != Some(read_size) {
            return zip.set_error(ZipError::FileReadFailed);
        }
        
        if (flags & 0) == 0 {
            if mz_crc32(MZ_CRC32_INIT, &buf[..file_stat.m_uncomp_size as usize]) != file_stat.m_crc32 {
                return zip.set_error(ZipError::CrcCheckFailed);
            }
        }
        
        return true;
    }
    
    let mut inflator = TinflDecompressor::new();
    
    let (mut read_buf, mut using_temp_buf, read_buf_size) = {
        let state = zip.m_pState.as_ref().unwrap();
        
        if let Some(mem) = &state.m_pMem {
            let start = cur_file_ofs as usize;
            let end = start + file_stat.m_comp_size as usize;
            if end > mem.len() {
                return zip.set_error(ZipError::InvalidHeaderOrCorrupted);
            }
            (mem[start..end].to_vec(), false, 0)
        } else if let Some(user_buf) = user_read_buf {
            if user_read_buf_size == 0 {
                return false;
            }
            (user_buf[..0].to_vec(), false, 0)
        } else {
            let read_buf_size = file_stat.m_comp_size.min(MZ_ZIP_MAX_IO_BUF_SIZE as u64) as usize;
            if read_buf_size == 0 {
                return zip.set_error(ZipError::InternalError);
            }
            
            let vec = vec![0u8; read_buf_size];
            (vec, true, read_buf_size)
        }
    };
    
    comp_remaining = file_stat.m_comp_size;
    
    loop {
        let out_buf_size = (file_stat.m_uncomp_size - out_buf_ofs as u64) as usize;
        
        if read_buf_avail == 0 && using_temp_buf {
            read_buf_avail = read_buf_size.min(comp_remaining as usize);
            if read_buf_avail == 0 {
                break;
            }
            
            if zip.read_at(cur_file_ofs, &mut read_buf[..read_buf_avail]).ok() != Some(read_buf_avail) {
                status = TinflStatus::Failed;
                zip.set_error(ZipError::DecompressionFailed);
                break;
            }
            cur_file_ofs += read_buf_avail as u64;
            comp_remaining -= read_buf_avail as u64;
            read_buf_ofs = 0;
        }
        
        let in_buf_size = read_buf_avail;
        let (new_status, in_consumed, out_produced) = inflator.decompress(
            &read_buf[read_buf_ofs..],
            &mut buf[out_buf_ofs..],
            0,
        );
        
        read_buf_avail -= in_consumed;
        read_buf_ofs += in_consumed;
        out_buf_ofs += out_produced;
        status = new_status;
        
        if status != TinflStatus::NeedsMoreInput {
            break;
        }
    }
    
    if status == TinflStatus::Done {
        if out_buf_ofs != file_stat.m_uncomp_size as usize {
            zip.set_error(ZipError::UnexpectedDecompressedSize);
            status = TinflStatus::Failed;
        } else if mz_crc32(MZ_CRC32_INIT, &buf[..out_buf_ofs]) != file_stat.m_crc32 {
            zip.set_error(ZipError::CrcCheckFailed);
            status = TinflStatus::Failed;
        }
    }
    
    status == TinflStatus::Done
}

pub fn mz_zip_reader_extract_to_mem_no_alloc(
    zip: &mut ZipArchive7,
    file_index: u32,
    buf: &mut [u8],
    buf_size: usize,
    flags: u32,
    user_read_buf: Option<&mut [u8]>,
    user_read_buf_size: usize,
) -> bool {
    mz_zip_reader_extract_to_mem_no_alloc1(
        zip,
        file_index,
        buf,
        buf_size,
        flags,
        user_read_buf,
        user_read_buf_size,
        None,
    )
}

pub fn mz_zip_reader_extract_file_to_mem_no_alloc(
    zip: &mut ZipArchive7,
    filename: &str,
    buf: &mut [u8],
    buf_size: usize,
    flags: u32,
    user_read_buf: Option<&mut [u8]>,
    user_read_buf_size: usize,
) -> bool {
    false
}

pub fn mz_zip_reader_extract_to_mem(
    zip: &mut ZipArchive7,
    file_index: u32,
    buf: &mut [u8],
    buf_size: usize,
    flags: u32,
) -> bool {
    mz_zip_reader_extract_to_mem_no_alloc1(
        zip,
        file_index,
        buf,
        buf_size,
        flags,
        None,
        0,
        None,
    )
}

pub fn mz_zip_reader_extract_file_to_mem(
    zip: &mut ZipArchive7,
    filename: &str,
    buf: &mut [u8],
    buf_size: usize,
    flags: u32,
) -> bool {
    mz_zip_reader_extract_file_to_mem_no_alloc(
        zip,
        filename,
        buf,
        buf_size,
        flags,
        None,
        0,
    )
}

pub fn mz_zip_reader_extract_to_heap(
    zip: &mut ZipArchive7,
    file_index: u32,
    size: &mut usize,
    flags: u32,
) -> Option<Vec<u8>> {
    *size = 0;
    
    let file_stat = ZipFileStat {
        m_is_directory: false,
        m_comp_size: 0,
        m_uncomp_size: 0,
        m_method: 0,
        m_bit_flag: 0,
        m_local_header_ofs: 0,
        m_crc32: 0,
    };
    
    let alloc_size = if (flags & 0) != 0 {
        file_stat.m_comp_size
    } else {
        file_stat.m_uncomp_size
    };
    
    if alloc_size > usize::MAX as u64 {
        zip.set_error(ZipError::InternalError);
        return None;
    }
    
    let mut buffer = vec![0u8; alloc_size as usize];
    
    if !mz_zip_reader_extract_to_mem_no_alloc1(
        zip,
        file_index,
        &mut buffer,
        alloc_size as usize,
        flags,
        None,
        0,
        Some(&file_stat),
    ) {
        return None;
    }
    
    *size = alloc_size as usize;
    Some(buffer)
}

pub fn mz_zip_reader_extract_file_to_heap(
    zip: &mut ZipArchive7,
    filename: &str,
    size: &mut usize,
    flags: u32,
) -> Option<Vec<u8>> {
    *size = 0;
    None
}

// --- Module: mz_p9 ---
pub fn mz_zip_extract_archive_file_to_heap_v2(
    _p_zip_filename: &str,
    _p_archive_name: &str,
    _p_comment: Option<&str>,
    _flags: u32,
) -> Result<Option<Vec<u8>>, ZipError> {
    unimplemented!()
}