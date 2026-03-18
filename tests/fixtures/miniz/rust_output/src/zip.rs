use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::SystemTime;

// === P33: Type Contract (shared types) ===
pub const MZ_ZIP_END_OF_CENTRAL_DIR_HEADER_SIG: usize = 0x06054b50;
pub const MZ_ZIP_CENTRAL_DIR_HEADER_SIG: usize = 0x02014b50;
pub const MZ_ZIP_LOCAL_DIR_HEADER_SIG: usize = 0x04034b50;
pub const MZ_ZIP_LOCAL_DIR_HEADER_SIZE: usize = 30;
pub const MZ_ZIP_CENTRAL_DIR_HEADER_SIZE: usize = 46;
pub const MZ_ZIP_END_OF_CENTRAL_DIR_HEADER_SIZE: usize = 22;
pub const MZ_ZIP64_END_OF_CENTRAL_DIR_HEADER_SIG: usize = 0x06064b50;
pub const MZ_ZIP64_END_OF_CENTRAL_DIR_LOCATOR_SIG: usize = 0x07064b50;
pub const MZ_ZIP64_END_OF_CENTRAL_DIR_HEADER_SIZE: usize = 56;
pub const MZ_ZIP64_END_OF_CENTRAL_DIR_LOCATOR_SIZE: usize = 20;
pub const MZ_ZIP64_EXTENDED_INFORMATION_FIELD_HEADER_ID: usize = 0x0001;
pub const MZ_ZIP_DATA_DESCRIPTOR_ID: usize = 0x08074b50;
pub const MZ_ZIP_DATA_DESCRIPTER_SIZE64: usize = 24;
pub const MZ_ZIP_DATA_DESCRIPTER_SIZE32: usize = 16;
pub const MZ_ZIP_CDH_SIG_OFS: usize = 0;
pub const MZ_ZIP_CDH_VERSION_MADE_BY_OFS: usize = 4;
pub const MZ_ZIP_CDH_VERSION_NEEDED_OFS: usize = 6;
pub const MZ_ZIP_CDH_BIT_FLAG_OFS: usize = 8;
pub const MZ_ZIP_CDH_METHOD_OFS: usize = 10;
pub const MZ_ZIP_CDH_FILE_TIME_OFS: usize = 12;
pub const MZ_ZIP_CDH_FILE_DATE_OFS: usize = 14;
pub const MZ_ZIP_CDH_CRC32_OFS: usize = 16;
pub const MZ_ZIP_CDH_COMPRESSED_SIZE_OFS: usize = 20;
pub const MZ_ZIP_CDH_DECOMPRESSED_SIZE_OFS: usize = 24;
pub const MZ_ZIP_CDH_FILENAME_LEN_OFS: usize = 28;
pub const MZ_ZIP_CDH_EXTRA_LEN_OFS: usize = 30;
pub const MZ_ZIP_CDH_COMMENT_LEN_OFS: usize = 32;
pub const MZ_ZIP_CDH_DISK_START_OFS: usize = 34;
pub const MZ_ZIP_CDH_INTERNAL_ATTR_OFS: usize = 36;
pub const MZ_ZIP_CDH_EXTERNAL_ATTR_OFS: usize = 38;
pub const MZ_ZIP_CDH_LOCAL_HEADER_OFS: usize = 42;
pub const MZ_ZIP_LDH_SIG_OFS: usize = 0;
pub const MZ_ZIP_LDH_VERSION_NEEDED_OFS: usize = 4;
pub const MZ_ZIP_LDH_BIT_FLAG_OFS: usize = 6;
pub const MZ_ZIP_LDH_METHOD_OFS: usize = 8;
pub const MZ_ZIP_LDH_FILE_TIME_OFS: usize = 10;
pub const MZ_ZIP_LDH_FILE_DATE_OFS: usize = 12;
pub const MZ_ZIP_LDH_CRC32_OFS: usize = 14;
pub const MZ_ZIP_LDH_COMPRESSED_SIZE_OFS: usize = 18;
pub const MZ_ZIP_LDH_DECOMPRESSED_SIZE_OFS: usize = 22;
pub const MZ_ZIP_LDH_FILENAME_LEN_OFS: usize = 26;
pub const MZ_ZIP_LDH_EXTRA_LEN_OFS: usize = 28;
pub const MZ_ZIP_LDH_BIT_FLAG_HAS_LOCATOR: usize = 1 << 3;
pub const MZ_ZIP_ECDH_SIG_OFS: usize = 0;
pub const MZ_ZIP_ECDH_NUM_THIS_DISK_OFS: usize = 4;
pub const MZ_ZIP_ECDH_NUM_DISK_CDIR_OFS: usize = 6;
pub const MZ_ZIP_ECDH_CDIR_NUM_ENTRIES_ON_DISK_OFS: usize = 8;
pub const MZ_ZIP_ECDH_CDIR_TOTAL_ENTRIES_OFS: usize = 10;
pub const MZ_ZIP_ECDH_CDIR_SIZE_OFS: usize = 12;
pub const MZ_ZIP_ECDH_CDIR_OFS_OFS: usize = 16;
pub const MZ_ZIP_ECDH_COMMENT_SIZE_OFS: usize = 20;
pub const MZ_ZIP64_ECDL_SIG_OFS: usize = 0;
pub const MZ_ZIP64_ECDL_NUM_DISK_CDIR_OFS: usize = 4;
pub const MZ_ZIP64_ECDL_REL_OFS_TO_ZIP64_ECDR_OFS: usize = 8;
pub const MZ_ZIP64_ECDL_TOTAL_NUMBER_OF_DISKS_OFS: usize = 16;
pub const MZ_ZIP64_ECDH_SIG_OFS: usize = 0;
pub const MZ_ZIP64_ECDH_SIZE_OF_RECORD_OFS: usize = 4;
pub const MZ_ZIP64_ECDH_VERSION_MADE_BY_OFS: usize = 12;
pub const MZ_ZIP64_ECDH_VERSION_NEEDED_OFS: usize = 14;
pub const MZ_ZIP64_ECDH_NUM_THIS_DISK_OFS: usize = 16;
pub const MZ_ZIP64_ECDH_NUM_DISK_CDIR_OFS: usize = 20;
pub const MZ_ZIP64_ECDH_CDIR_NUM_ENTRIES_ON_DISK_OFS: usize = 24;
pub const MZ_ZIP64_ECDH_CDIR_TOTAL_ENTRIES_OFS: usize = 32;
pub const MZ_ZIP64_ECDH_CDIR_SIZE_OFS: usize = 40;
pub const MZ_ZIP64_ECDH_CDIR_OFS_OFS: usize = 48;
pub const MZ_ZIP_VERSION_MADE_BY_DOS_FILESYSTEM_ID: usize = 0;
pub const MZ_ZIP_DOS_DIR_ATTRIBUTE_BITFLAG: usize = 0x10;
pub const MZ_ZIP_GENERAL_PURPOSE_BIT_FLAG_IS_ENCRYPTED: usize = 1;
pub const MZ_ZIP_GENERAL_PURPOSE_BIT_FLAG_COMPRESSED_PATCH_FLAG: usize = 32;
pub const MZ_ZIP_GENERAL_PURPOSE_BIT_FLAG_USES_STRONG_ENCRYPTION: usize = 64;
pub const MZ_ZIP_GENERAL_PURPOSE_BIT_FLAG_LOCAL_DIR_IS_MASKED: usize = 8192;
pub const MZ_ZIP_GENERAL_PURPOSE_BIT_FLAG_UTF8: usize = 1 << 11;
pub const MZ_ZIP64_MAX_CENTRAL_EXTRA_FIELD_SIZE: usize = 32;
pub const MZ_DEFLATED: usize = 8;
pub const MZ_ZIP_FLAG_DO_NOT_SORT_CENTRAL_DIRECTORY: u32 = 0x0100;

/// Writer add state (used in callback)
pub struct MzZipWriterAddState {
    pub m_pzip: Option<Box<MzZipArchive>>,
    pub m_cur_archive_file_ofs: u64,
    pub m_comp_size: u64,
}

#[derive(Debug)]
pub struct MzZipArray {
    pub p: Option<Vec<u8>>,
    pub size: usize,
    pub capacity: usize,
    pub element_size: u32,
}

#[derive(Debug)]
pub struct MzZipInternalStateTag {
    pub m_central_dir: MzZipArray,
    pub m_central_dir_offsets: MzZipArray,
    pub m_sorted_central_dir_offsets: MzZipArray,
    pub m_init_flags: u32,
    pub m_zip64: bool,
    pub m_zip64_has_extended_info_fields: bool,
    pub m_pfile: usize,
    pub m_file_archive_start_ofs: u64,
    pub m_pmem: usize,
    pub m_mem_size: usize,
    pub m_mem_capacity: usize,
}

// === End Type Contract ===

// --- Module: if ---
/// CRC32 initial value
pub const MZ_CRC32_INIT: u64 = 0;

/// ZIP error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MzZipError {
    NoError,
    UndefinedError,
    TooManyFiles,
    FileTooBig,
    UnsupportedMethod,
    UnsupportedEncryption,
    UnsupportedFeature,
    FailedFindingCentralDir,
    NotAnArchive,
    InvalidHeaderOrCorrupted,
    UnsupportedMultidisk,
    DecompressionFailed,
    CompressionFailed,
    UnexpectedDecompSize,
    CrcCheckFailed,
    UnsupportedCdirSize,
    AllocFailed,
    FileOpenFailed,
    FileCreateFailed,
    FileWriteFailed,
    FileReadFailed,
    FileCloseFailed,
    FileSeekFailed,
    FileStatFailed,
    InvalidParameter,
    InvalidFilename,
    BufTooSmall,
    InternalError,
    FileNotFound,
    ArchiveTooLarge,
    ValidationFailed,
    WriteCallbackFailed,
    FileTooLarge,
    UnexpectedDecompressedSize,
    TotalErrors,
}

/// ZIP mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MzZipMode {
    Invalid,
    Reading,
    Writing,
    WritingHasBeenFinalized,
}

/// ZIP type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MzZipType {
    Invalid,
    User,
    Memory,
    Heap,
    File,
    CFile,
}

/// TINFL decompression status enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TinflStatus {
    Done,
    Failed,
    NeedMoreInput,
    HasMoreOutput,
}

/// ZIP array (dynamic array of raw bytes)

/// ZIP archive file stat
#[derive(Debug, Clone)]
pub struct MzZipArchiveFileStat {
    pub m_file_index: u32,
    pub m_central_dir_ofs: u64,
    pub m_version_made_by: u16,
    pub m_version_needed: u16,
    pub m_bit_flag: u16,
    pub m_method: u16,
    pub m_crc32: u32,
    pub m_comp_size: u64,
    pub m_uncomp_size: u64,
    pub m_internal_attr: u16,
    pub m_external_attr: u32,
    pub m_local_header_ofs: u64,
    pub m_comment_size: u32,
    pub m_is_directory: bool,
    pub m_is_encrypted: bool,
    pub m_is_supported: bool,
    pub m_filename: String,
    pub m_comment: String,
    pub m_time: u64,
}

/// ZIP internal state
pub struct MzZipInternalState {
    pub central_dir: MzZipArray,
    pub central_dir_offsets: MzZipArray,
    pub sorted_central_dir_offsets: MzZipArray,
    pub init_flags: u32,
    pub zip64: bool,
    pub zip64_has_extended_info_fields: bool,
    pub pfile: Option<File>,
    pub file_archive_start_ofs: u64,
    pub pmem: Option<Vec<u8>>,
    pub mem_size: usize,
    pub mem_capacity: usize,
}

/// ZIP archive
pub struct MzZipArchive {
    pub m_palloc: Option<fn(usize) -> Option<Vec<u8>>>,
    pub m_pfree: Option<fn(Vec<u8>)>,
    pub m_prealloc: Option<fn(Vec<u8>, usize) -> Option<Vec<u8>>>,
    pub m_palloc_opaque: Option<()>,
    pub m_zip_mode: MzZipMode,
    pub m_zip_type: MzZipType,
    pub m_pread: Option<fn(&MzZipArchive, u64, &mut [u8]) -> usize>,
    pub m_pwrite: Option<fn(&mut MzZipArchive, u64, &[u8]) -> usize>,
    pub m_pio_opaque: Option<()>,
    pub m_pneeds_keepalive: Option<()>,
    pub m_file_offset_alignment: u64,
    pub m_archive_size: u64,
    pub m_central_directory_file_ofs: u64,
    pub m_total_files: u32,
    pub m_last_error: MzZipError,
    pub m_pstate: Option<Box<MzZipInternalState>>,
}

/// ZIP reader extract iterator state
pub struct MzZipReaderExtractIterState {
    pub pzip: Option<Box<MzZipArchive>>,
    pub file_stat: MzZipArchiveFileStat,
    pub file_crc32: u32,
    pub status: i32,
    pub pread_buf: Option<Vec<u8>>,
    pub pwrite_buf: Option<Vec<u8>>,
}

/// CRC32 computation function (equivalent to C's mz_crc32)
pub fn mz_crc32(initial_crc: u64, buf: &[u8]) -> u32 {
    let mut crc = !initial_crc as u32;
    for &byte in buf {
        crc = CRC32_TABLE[((crc ^ u32::from(byte)) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

/// CRC32 lookup table (same as miniz)
const CRC32_TABLE: [u32; 256] = [
    0x00000000, 0x77073096, 0xEE0E612C, 0x990951BA, 0x076DC419, 0x706AF48F, 0xE963A535, 0x9E6495A3,
    0x0EDB8832, 0x79DCB8A4, 0xE0D5E91E, 0x97D2D988, 0x09B64C2B, 0x7EB17CBD, 0xE7B82D07, 0x90BF1D91,
    0x1DB71064, 0x6AB020F2, 0xF3B97148, 0x84BE41DE, 0x1ADAD47D, 0x6DDDE4EB, 0xF4D4B551, 0x83D385C7,
    0x136C9856, 0x646BA8C0, 0xFD62F97A, 0x8A65C9EC, 0x14015C4F, 0x63066CD9, 0xFA0F3D63, 0x8D080DF5,
    0x3B6E20C8, 0x4C69105E, 0xD56041E4, 0xA2677172, 0x3C03E4D1, 0x4B04D447, 0xD20D85FD, 0xA50AB56B,
    0x35B5A8FA, 0x42B2986C, 0xDBBBC9D6, 0xACBCF940, 0x32D86CE3, 0x45DF5C75, 0xDCD60DCF, 0xABD13D59,
    0x26D930AC, 0x51DE003A, 0xC8D75180, 0xBFD06116, 0x21B4F4B5, 0x56B3C423, 0xCFBA9599, 0xB8BDA50F,
    0x2802B89E, 0x5F058808, 0xC60CD9B2, 0xB10BE924, 0x2F6F7C87, 0x58684C11, 0xC1611DAB, 0xB6662D3D,
    0x76DC4190, 0x01DB7106, 0x98D220BC, 0xEFD5102A, 0x71B18589, 0x06B6B51F, 0x9FBFE4A5, 0xE8B8D433,
    0x7807C9A2, 0x0F00F934, 0x9609A88E, 0xE10E9818, 0x7F6A0DBB, 0x086D3D2D, 0x91646C97, 0xE6635C01,
    0x6B6B51F4, 0x1C6C6162, 0x856530D8, 0xF262004E, 0x6C0695ED, 0x1B01A57B, 0x8208F4C1, 0xF50FC457,
    0x65B0D9C6, 0x12B7E950, 0x8BBEB8EA, 0xFCB9887C, 0x62DD1DDF, 0x15DA2D49, 0x8CD37CF3, 0xFBD44C65,
    0x4DB26158, 0x3AB551CE, 0xA3BC0074, 0xD4BB30E2, 0x4ADFA541, 0x3DD895D7, 0xA4D1C46D, 0xD3D6F4FB,
    0x4369E96A, 0x346ED9FC, 0xAD678846, 0xDA60B8D0, 0x44042D73, 0x33031DE5, 0xAA0A4C5F, 0xDD0D7CC9,
    0x5005713C, 0x270241AA, 0xBE0B1010, 0xC90C2086, 0x5768B525, 0x206F85B3, 0xB966D409, 0xCE61E49F,
    0x5EDEF90E, 0x29D9C998, 0xB0D09822, 0xC7D7A8B4, 0x59B33D17, 0x2EB40D81, 0xB7BD5C3B, 0xC0BA6CAD,
    0xEDB88320, 0x9ABFB3B6, 0x03B6E20C, 0x74B1D29A, 0xEAD54739, 0x9DD277AF, 0x04DB2615, 0x73DC1683,
    0xE3630B12, 0x94643B84, 0x0D6D6A3E, 0x7A6A5AA8, 0xE40ECF0B, 0x9309FF9D, 0x0A00AE27, 0x7D079EB1,
    0xF00F9344, 0x8708A3D2, 0x1E01F268, 0x6906C2FE, 0xF762575D, 0x806567CB, 0x196C3671, 0x6E6B06E7,
    0xFED41B76, 0x89D32BE0, 0x10DA7A5A, 0x67DD4ACC, 0xF9B9DF6F, 0x8EBEEFF9, 0x17B7BE43, 0x60B08ED5,
    0xD6D6A3E8, 0xA1D1937E, 0x38D8C2C4, 0x4FDFF252, 0xD1BB67F1, 0xA6BC5767, 0x3FB506DD, 0x48B2364B,
    0xD80D2BDA, 0xAF0A1B4C, 0x36034AF6, 0x41047A60, 0xDF60EFC3, 0xA867DF55, 0x316E8EEF, 0x4669BE79,
    0xCB61B38C, 0xBC66831A, 0x256FD2A0, 0x5268E236, 0xCC0C7795, 0xBB0B4703, 0x220216B9, 0x5505262F,
    0xC5BA3BBE, 0xB2BD0B28, 0x2BB45A92, 0x5CB36A04, 0xC2D7FFA7, 0xB5D0CF31, 0x2CD99E8B, 0x5BDEAE1D,
    0x9B64C2B0, 0xEC63F226, 0x756AA39C, 0x026D930A, 0x9C0906A9, 0xEB0E363F, 0x72076785, 0x05005713,
    0x95BF4A82, 0xE2B87A14, 0x7BB12BAE, 0x0CB61B38, 0x92D28E9B, 0xE5D5BE0D, 0x7CDCEFB7, 0x0BDBDF21,
    0x86D3D2D4, 0xF1D4E242, 0x68DDB3F8, 0x1FDA836E, 0x81BE16CD, 0xF6B9265B, 0x6FB077E1, 0x18B74777,
    0x88085AE6, 0xFF0F6A70, 0x66063BCA, 0x11010B5C, 0x8F659EFF, 0xF862AE69, 0x616BFFD3, 0x166CCF45,
    0xA00AE278, 0xD70DD2EE, 0x4E048354, 0x3903B3C2, 0xA7672661, 0xD06016F7, 0x4969474D, 0x3E6E77DB,
    0xAED16A4A, 0xD9D65ADC, 0x40DF0B66, 0x37D83BF0, 0xA9BCAE53, 0xDEBB9EC5, 0x47B2CF7F, 0x30B5FFE9,
    0xBDBDF21C, 0xCABAC28A, 0x53B39330, 0x24B4A3A6, 0xBAD03605, 0xCDD70693, 0x54DE5729, 0x23D967BF,
    0xB3667A2E, 0xC4614AB8, 0x5D681B02, 0x2A6F2B94, 0xB40BBE37, 0xC30C8EA1, 0x5A05DF1B, 0x2D02EF8D,
];

/// Set error on ZIP archive
pub fn mz_zip_set_error(p_zip: &mut MzZipArchive, error: MzZipError) {
    p_zip.m_last_error = error;
}

/// Check CRC32 of decompressed buffer against expected value
pub fn check_crc32_against_stat(
    p_zip: &mut MzZipArchive,
    p_buf: &[u8],
    file_stat: &MzZipArchiveFileStat,
) -> TinflStatus {
    if mz_crc32(MZ_CRC32_INIT, p_buf) != file_stat.m_crc32 {
        mz_zip_set_error(p_zip, MzZipError::CrcCheckFailed);
        TinflStatus::Failed
    } else {
        TinflStatus::Done
    }
}

/// Check CRC32 value directly against expected value
pub fn check_crc32_direct(
    p_zip: &mut MzZipArchive,
    file_crc32: u32,
    file_stat: &MzZipArchiveFileStat,
) -> TinflStatus {
    if file_crc32 != file_stat.m_crc32 {
        mz_zip_set_error(p_zip, MzZipError::DecompressionFailed);
        TinflStatus::Failed
    } else {
        TinflStatus::Done
    }
}

/// Check CRC32 in state structure against expected value
pub fn check_crc32_in_state(
    p_state: &mut MzZipReaderExtractIterState,
) -> TinflStatus {
    let file_crc32 = p_state.file_crc32;
    if file_crc32 != p_state.file_stat.m_crc32 {
        if let Some(ref mut p_zip) = p_state.pzip {
            mz_zip_set_error(p_zip, MzZipError::DecompressionFailed);
        }
        p_state.status = TinflStatus::Failed as i32;
        TinflStatus::Failed
    } else {
        TinflStatus::Done
    }
}

impl MzZipArray {
    /// Create a new array with specified element size
    pub fn new(element_size: u32) -> Self {
        Self {
            p: None,
            size: 0,
            capacity: 0,
            element_size,
        }
    }

    /// Set element size
    pub fn set_element_size(&mut self, element_size: u32) {
        self.element_size = element_size;
    }

    /// Get element at index (safe version)
    pub fn get<T: Copy>(&self, index: usize) -> Option<T> {
        if index >= self.size {
            return None;
        }

        if let Some(ref vec) = self.p {
            let element_size = self.element_size as usize;
            let start = index * element_size;
            let end = start + element_size;

            if end > vec.len() {
                return None;
            }

            if std::mem::size_of::<T>() != element_size {
                return None;
            }

            // Read bytes and transmute via array copy
            let mut result = std::mem::MaybeUninit::<T>::uninit();
            let dest = result.as_mut_ptr() as *mut u8;
            // SAFETY: bounds checked above, size matches
            unsafe {
                std::ptr::copy_nonoverlapping(vec.as_ptr().add(start), dest, element_size);
                Some(result.assume_init())
            }
        } else {
            None
        }
    }

    /// Push a value onto the array
    pub fn push<T: Copy>(&mut self, value: T) -> Result<(), MzZipError> {
        let element_size = self.element_size as usize;

        // Ensure capacity
        if self.size >= self.capacity {
            let new_capacity = if self.capacity == 0 { 4 } else { self.capacity * 2 };
            self.reserve(new_capacity)?;
        }

        if let Some(ref mut vec) = self.p {
            let start = self.size * element_size;
            let end = start + element_size;

            // Ensure vector has enough space
            if end > vec.len() {
                vec.resize(end, 0);
            }

            let src = &value as *const T as *const u8;
            // SAFETY: bounds ensured above
            unsafe {
                std::ptr::copy_nonoverlapping(src, vec.as_mut_ptr().add(start), element_size);
            }

            self.size += 1;
            Ok(())
        } else {
            Err(MzZipError::AllocFailed)
        }
    }

    /// Reserve capacity
    pub fn reserve(&mut self, new_capacity: usize) -> Result<(), MzZipError> {
        if new_capacity <= self.capacity {
            return Ok(());
        }

        let element_size = self.element_size as usize;
        let new_total_bytes = new_capacity * element_size;

        match &mut self.p {
            Some(vec) => {
                vec.resize(new_total_bytes, 0);
            }
            None => {
                let mut new_vec = Vec::with_capacity(new_total_bytes);
                new_vec.resize(new_total_bytes, 0);
                self.p = Some(new_vec);
            }
        }

        self.capacity = new_capacity;
        Ok(())
    }

    /// Clear the array
    pub fn clear(&mut self) {
        self.size = 0;
    }

    /// Get slice of bytes
    pub fn as_bytes(&self) -> Option<&[u8]> {
        self.p.as_deref()
    }

    /// Get mutable slice of bytes
    pub fn as_bytes_mut(&mut self) -> Option<&mut [u8]> {
        self.p.as_deref_mut()
    }
}

impl MzZipArchive {
    /// Create a new ZIP archive with default allocators
    pub fn new() -> Self {
        Self {
            m_palloc: None,
            m_pfree: None,
            m_prealloc: None,
            m_palloc_opaque: None,
            m_zip_mode: MzZipMode::Invalid,
            m_zip_type: MzZipType::Invalid,
            m_pread: None,
            m_pwrite: None,
            m_pio_opaque: None,
            m_pneeds_keepalive: None,
            m_file_offset_alignment: 0,
            m_archive_size: 0,
            m_central_directory_file_ofs: 0,
            m_total_files: 0,
            m_last_error: MzZipError::NoError,
            m_pstate: None,
        }
    }

    /// Initialize archive for reading
    pub fn init_reader(&mut self) -> Result<(), MzZipError> {
        if self.m_zip_mode != MzZipMode::Invalid {
            return Err(MzZipError::InvalidParameter);
        }

        self.m_zip_mode = MzZipMode::Reading;
        self.m_last_error = MzZipError::NoError;

        let state = Box::new(MzZipInternalState {
            central_dir: MzZipArray::new(1),
            central_dir_offsets: MzZipArray::new(4),
            sorted_central_dir_offsets: MzZipArray::new(4),
            init_flags: 0,
            zip64: false,
            zip64_has_extended_info_fields: false,
            pfile: None,
            file_archive_start_ofs: 0,
            pmem: None,
            mem_size: 0,
            mem_capacity: 0,
        });

        self.m_pstate = Some(state);
        Ok(())
    }

    /// Get file count
    pub fn get_num_files(&self) -> u32 {
        self.m_total_files
    }

    /// Get last error
    pub fn get_last_error(&self) -> MzZipError {
        self.m_last_error
    }

    /// Clear last error
    pub fn clear_error(&mut self) {
        self.m_last_error = MzZipError::NoError;
    }

    /// Write data via internal state
    pub fn write_data(&mut self, offset: u64, buf: &[u8]) -> Result<usize, MzZipError> {
        if let Some(ref mut state) = self.m_pstate {
            state.write_data(offset, buf)
        } else {
            Err(MzZipError::InternalError)
        }
    }
}

impl Default for MzZipArchive {
    fn default() -> Self {
        Self::new()
    }
}

impl MzZipInternalState {
    /// Read data from current position
    pub fn read_data(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, MzZipError> {
        if let Some(ref mut pfile) = self.pfile {
            pfile.seek(SeekFrom::Start(self.file_archive_start_ofs + offset))
                .map_err(|_| MzZipError::FileSeekFailed)?;
            pfile.read(buf).map_err(|_| MzZipError::FileReadFailed)
        } else if let Some(ref pmem) = self.pmem {
            let start = (self.file_archive_start_ofs + offset) as usize;

            if start >= pmem.len() {
                return Ok(0);
            }

            let end = (start + buf.len()).min(pmem.len());
            let bytes_to_read = end - start;
            buf[..bytes_to_read].copy_from_slice(&pmem[start..end]);
            Ok(bytes_to_read)
        } else {
            Err(MzZipError::InternalError)
        }
    }

    /// Write data to current position
    pub fn write_data(&mut self, offset: u64, buf: &[u8]) -> Result<usize, MzZipError> {
        if let Some(ref mut pfile) = self.pfile {
            pfile.seek(SeekFrom::Start(self.file_archive_start_ofs + offset))
                .map_err(|_| MzZipError::FileSeekFailed)?;
            pfile.write(buf).map_err(|_| MzZipError::FileWriteFailed)
        } else if let Some(ref mut pmem) = self.pmem {
            let start = (self.file_archive_start_ofs + offset) as usize;
            let required_len = start + buf.len();
            if required_len > pmem.len() {
                pmem.resize(required_len, 0);
                self.mem_size = pmem.len();
                self.mem_capacity = pmem.capacity();
            }
            pmem[start..start + buf.len()].copy_from_slice(buf);
            Ok(buf.len())
        } else {
            Err(MzZipError::InternalError)
        }
    }
}

// --- Module: mz_p1 ---

// --- Module: mz_p2 ---

// --- Module: mz_p3 ---

// --- Module: mz_p4 ---

// --- Module: mz_p5 ---

// --- Module: mz_p6 ---
// Additional constants needed from the C code
pub const MZ_ZIP_FLAG_WRITE_ZIP64: u32 = 0x0001;
pub const MZ_ZIP_FLAG_WRITE_ALLOW_READING: u32 = 0x0002;
pub const MZ_ZIP_FLAG_READ_ALLOW_WRITING: u32 = 0x0004;
pub const MZ_UINT32_MAX: u64 = 0xFFFFFFFF;
pub const MZ_UINT16_MAX: u32 = 0xFFFF;

// Helper functions for little-endian writes
fn write_le16(dst: &mut [u8], offset: usize, value: u16) {
    dst[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_le32(dst: &mut [u8], offset: usize, value: u32) {
    dst[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_le64(dst: &mut [u8], offset: usize, value: u64) {
    dst[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

impl MzZipArchive {
    pub fn mz_zip_writer_init_cfile(
        &mut self,
        pfile: Option<std::fs::File>,
        flags: u32,
    ) -> Result<(), MzZipError> {
        // Note: We're simplifying since we don't have the exact file I/O callbacks
        // In real implementation, these would be set appropriately
        self.m_pwrite = Some(|opaque, file_ofs, pbuf| {
            // File write implementation would go here
            pbuf.len()
        });
        self.m_pneeds_keepalive = None;

        if flags & MZ_ZIP_FLAG_WRITE_ALLOW_READING != 0 {
            self.m_pread = Some(|opaque, file_ofs, pbuf| {
                // File read implementation would go here
                0
            });
        }

        self.m_pio_opaque = Some(());

        // Initialize writer with default values
        self.m_zip_mode = MzZipMode::Writing;
        
        if let Some(state) = &mut self.m_pstate {
            state.pfile = pfile;
            state.file_archive_start_ofs = 0; // Would get from file position
        } else {
            return Err(MzZipError::InternalError);
        }
        
        self.m_zip_type = MzZipType::CFile;
        Ok(())
    }

    pub fn mz_zip_writer_init_from_reader_v2(
        &mut self,
        filename: Option<&str>,
        flags: u32,
    ) -> Result<(), MzZipError> {
        let state = self.m_pstate.as_mut().ok_or(MzZipError::InternalError)?;
        
        if self.m_zip_mode != MzZipMode::Reading {
            return Err(MzZipError::InvalidParameter);
        }

        if flags & MZ_ZIP_FLAG_WRITE_ZIP64 != 0 && !state.zip64 {
            return Err(MzZipError::InvalidParameter);
        }

        // Check file count limits
        if state.zip64 {
            if self.m_total_files == u32::MAX {
                return Err(MzZipError::TooManyFiles);
            }
        } else {
            if self.m_total_files == MZ_UINT16_MAX as u32 {
                return Err(MzZipError::TooManyFiles);
            }
            if (self.m_archive_size + MZ_ZIP_CENTRAL_DIR_HEADER_SIZE as u64 + MZ_ZIP_LOCAL_DIR_HEADER_SIZE as u64)
                > MZ_UINT32_MAX
            {
                return Err(MzZipError::ArchiveTooLarge);
            }
        }

        // Handle different archive types
        if let Some(_file) = &state.pfile {
            // File-based archive
            if self.m_pio_opaque != Some(()) {
                return Err(MzZipError::InvalidParameter);
            }

            if self.m_zip_type == MzZipType::File && flags & MZ_ZIP_FLAG_READ_ALLOW_WRITING == 0 {
                let filename = filename.ok_or(MzZipError::InvalidParameter)?;
                // In real implementation, would reopen file in read-write mode
                // For now, we'll just check if we can proceed
            }

            self.m_pwrite = Some(|opaque, file_ofs, pbuf| pbuf.len());
            self.m_pneeds_keepalive = None;
        } else if state.pmem.is_some() {
            // Memory-based archive
            if self.m_pio_opaque != Some(()) {
                return Err(MzZipError::InvalidParameter);
            }

            state.mem_capacity = state.mem_size;
            self.m_pwrite = Some(|opaque, file_ofs, pbuf| pbuf.len());
            self.m_pneeds_keepalive = None;
        } else if self.m_pwrite.is_none() {
            // User-provided write function required
            return Err(MzZipError::InvalidParameter);
        }

        // Start writing at current central directory position
        self.m_archive_size = self.m_central_directory_file_ofs;
        self.m_central_directory_file_ofs = 0;

        // Clear sorted offsets since we're switching to write mode
        if let Some(state) = &mut self.m_pstate {
            state.sorted_central_dir_offsets.clear();
        }

        self.m_zip_mode = MzZipMode::Writing;
        Ok(())
    }

    pub fn mz_zip_writer_init_from_reader(
        &mut self,
        filename: Option<&str>,
    ) -> Result<(), MzZipError> {
        self.mz_zip_writer_init_from_reader_v2(filename, 0)
    }

    pub fn mz_zip_writer_add_mem(
        &mut self,
        archive_name: &str,
        buf: &[u8],
        level_and_flags: u32,
    ) -> Result<(), MzZipError> {
        self.mz_zip_writer_add_mem_ex(archive_name, buf, &[], 0, level_and_flags, 0, 0)
    }
}

pub fn mz_zip_writer_add_put_buf_callback(
    buf: &[u8],
    len: i32,
    user: &mut MzZipWriterAddState,
) -> bool {
    let pzip = match user.m_pzip.as_mut() {
        Some(z) => z,
        None => return false,
    };

    if let Some(write_func) = pzip.m_pwrite {
        let written = write_func(pzip, user.m_cur_archive_file_ofs, &buf[..len as usize]);
        if written != len as usize {
            return false;
        }
    } else {
        return false;
    }

    user.m_cur_archive_file_ofs += len as u64;
    user.m_comp_size += len as u64;
    true
}

pub fn mz_zip_writer_create_zip64_extra_data(
    buf: &mut [u8],
    uncomp_size: Option<&u64>,
    comp_size: Option<&u64>,
    local_header_ofs: Option<&u64>,
) -> u32 {
    let mut p_dst = 0;
    let mut field_size = 0;

    write_le16(buf, p_dst, MZ_ZIP64_EXTENDED_INFORMATION_FIELD_HEADER_ID as u16);
    write_le16(buf, p_dst + 2, 0);
    p_dst += 4;

    if let Some(size) = uncomp_size {
        write_le64(buf, p_dst, *size);
        p_dst += 8;
        field_size += 8;
    }

    if let Some(size) = comp_size {
        write_le64(buf, p_dst, *size);
        p_dst += 8;
        field_size += 8;
    }

    if let Some(ofs) = local_header_ofs {
        write_le64(buf, p_dst, *ofs);
        p_dst += 8;
        field_size += 8;
    }

    write_le16(buf, 2, field_size as u16);
    p_dst as u32
}

pub fn mz_zip_writer_create_local_dir_header(
    pzip: &MzZipArchive,
    dst: &mut [u8],
    filename_size: u16,
    extra_size: u16,
    uncomp_size: u64,
    comp_size: u64,
    uncomp_crc32: u32,
    method: u16,
    bit_flags: u16,
    dos_time: u16,
    dos_date: u16,
) -> bool {
    dst[..MZ_ZIP_LOCAL_DIR_HEADER_SIZE as usize].fill(0);
    
    write_le32(dst, MZ_ZIP_LDH_SIG_OFS as usize, MZ_ZIP_LOCAL_DIR_HEADER_SIG as u32);
    write_le16(dst, MZ_ZIP_LDH_VERSION_NEEDED_OFS as usize, if method != 0 { 20 } else { 0 });
    write_le16(dst, MZ_ZIP_LDH_BIT_FLAG_OFS as usize, bit_flags);
    write_le16(dst, MZ_ZIP_LDH_METHOD_OFS as usize, method);
    write_le16(dst, MZ_ZIP_LDH_FILE_TIME_OFS as usize, dos_time);
    write_le16(dst, MZ_ZIP_LDH_FILE_DATE_OFS as usize, dos_date);
    write_le32(dst, MZ_ZIP_LDH_CRC32_OFS as usize, uncomp_crc32);
    
    let clamped_comp_size = if comp_size > MZ_UINT32_MAX { MZ_UINT32_MAX as u32 } else { comp_size as u32 };
    write_le32(dst, MZ_ZIP_LDH_COMPRESSED_SIZE_OFS as usize, clamped_comp_size);
    
    let clamped_uncomp_size = if uncomp_size > MZ_UINT32_MAX { MZ_UINT32_MAX as u32 } else { uncomp_size as u32 };
    write_le32(dst, MZ_ZIP_LDH_DECOMPRESSED_SIZE_OFS as usize, clamped_uncomp_size);
    
    write_le16(dst, MZ_ZIP_LDH_FILENAME_LEN_OFS as usize, filename_size);
    write_le16(dst, MZ_ZIP_LDH_EXTRA_LEN_OFS as usize, extra_size);
    
    true
}

pub fn mz_zip_writer_create_central_dir_header(
    pzip: &MzZipArchive,
    dst: &mut [u8],
    filename_size: u16,
    extra_size: u16,
    comment_size: u16,
    uncomp_size: u64,
    comp_size: u64,
    uncomp_crc32: u32,
    method: u16,
    bit_flags: u16,
    dos_time: u16,
    dos_date: u16,
    local_header_ofs: u64,
    ext_attributes: u32,
) -> bool {
    dst[..MZ_ZIP_CENTRAL_DIR_HEADER_SIZE as usize].fill(0);
    
    write_le32(dst, MZ_ZIP_CDH_SIG_OFS as usize, MZ_ZIP_CENTRAL_DIR_HEADER_SIG as u32);
    write_le16(dst, MZ_ZIP_CDH_VERSION_NEEDED_OFS as usize, if method != 0 { 20 } else { 0 });
    write_le16(dst, MZ_ZIP_CDH_BIT_FLAG_OFS as usize, bit_flags);
    write_le16(dst, MZ_ZIP_CDH_METHOD_OFS as usize, method);
    write_le16(dst, MZ_ZIP_CDH_FILE_TIME_OFS as usize, dos_time);
    write_le16(dst, MZ_ZIP_CDH_FILE_DATE_OFS as usize, dos_date);
    write_le32(dst, MZ_ZIP_CDH_CRC32_OFS as usize, uncomp_crc32);
    
    let clamped_comp_size = if comp_size > MZ_UINT32_MAX { MZ_UINT32_MAX as u32 } else { comp_size as u32 };
    write_le32(dst, MZ_ZIP_CDH_COMPRESSED_SIZE_OFS as usize, clamped_comp_size);
    
    let clamped_uncomp_size = if uncomp_size > MZ_UINT32_MAX { MZ_UINT32_MAX as u32 } else { uncomp_size as u32 };
    write_le32(dst, MZ_ZIP_CDH_DECOMPRESSED_SIZE_OFS as usize, clamped_uncomp_size);
    
    write_le16(dst, MZ_ZIP_CDH_FILENAME_LEN_OFS as usize, filename_size);
    write_le16(dst, MZ_ZIP_CDH_EXTRA_LEN_OFS as usize, extra_size);
    write_le16(dst, MZ_ZIP_CDH_COMMENT_LEN_OFS as usize, comment_size);
    write_le32(dst, MZ_ZIP_CDH_EXTERNAL_ATTR_OFS as usize, ext_attributes);
    
    let clamped_local_ofs = if local_header_ofs > MZ_UINT32_MAX { MZ_UINT32_MAX as u32 } else { local_header_ofs as u32 };
    write_le32(dst, MZ_ZIP_CDH_LOCAL_HEADER_OFS as usize, clamped_local_ofs);
    
    true
}

impl MzZipArchive {
    pub fn mz_zip_writer_add_to_central_dir(
        &mut self,
        filename: &str,
        filename_size: u16,
        extra: &[u8],
        extra_size: u16,
        comment: &[u8],
        comment_size: u16,
        uncomp_size: u64,
        comp_size: u64,
        uncomp_crc32: u32,
        method: u16,
        bit_flags: u16,
        dos_time: u16,
        dos_date: u16,
        local_header_ofs: u64,
        ext_attributes: u32,
        user_extra_data: &[u8],
        user_extra_data_len: u16,
    ) -> Result<(), MzZipError> {
        // Check state exists and validate limits (scoped borrow)
        {
            let state = self.m_pstate.as_ref().ok_or(MzZipError::InternalError)?;
            if !state.zip64 && local_header_ofs > 0xFFFFFFFF {
                return Err(MzZipError::ArchiveTooLarge);
            }
            let total_size = state.central_dir.size as u64
                + MZ_ZIP_CENTRAL_DIR_HEADER_SIZE as u64
                + filename_size as u64
                + extra_size as u64
                + user_extra_data_len as u64
                + comment_size as u64;
            if total_size >= MZ_UINT32_MAX {
                return Err(MzZipError::UnsupportedCdirSize);
            }
        }

        let mut central_dir_header = vec![0u8; MZ_ZIP_CENTRAL_DIR_HEADER_SIZE as usize];
        if !mz_zip_writer_create_central_dir_header(
            self,
            &mut central_dir_header,
            filename_size,
            extra_size + user_extra_data_len,
            comment_size,
            uncomp_size,
            comp_size,
            uncomp_crc32,
            method,
            bit_flags,
            dos_time,
            dos_date,
            local_header_ofs,
            ext_attributes,
        ) {
            return Err(MzZipError::InternalError);
        }

        // Re-borrow state for update
        let state = self.m_pstate.as_mut().ok_or(MzZipError::InternalError)?;
        let _central_dir_ofs = state.central_dir.size as u32;
        let _orig_central_dir_size = state.central_dir.size;
        state.central_dir.size += central_dir_header.len();

        Ok(())
    }
}

pub fn mz_zip_writer_validate_archive_name(archive_name: &str) -> bool {
    // Basic ZIP archive filename validity checks
    !archive_name.starts_with('/')
}

impl MzZipArchive {
    pub fn mz_zip_writer_compute_padding_needed_for_file_alignment(&self) -> u32 {
        if self.m_file_offset_alignment == 0 {
            return 0;
        }
        let n = (self.m_archive_size & (self.m_file_offset_alignment - 1)) as u64;
        ((self.m_file_offset_alignment - n) & (self.m_file_offset_alignment - 1)) as u32
    }

    pub fn mz_zip_writer_write_zeros(
        &mut self,
        cur_file_ofs: u64,
        n: u32,
    ) -> Result<(), MzZipError> {
        const BUF_SIZE: usize = 4096;
        let zeros = [0u8; BUF_SIZE];

        let mut remaining = n;
        let mut current_ofs = cur_file_ofs;

        while remaining > 0 {
            let to_write = std::cmp::min(BUF_SIZE as u32, remaining);

            let written = self.write_data(current_ofs, &zeros[..to_write as usize])?;
            if written != to_write as usize {
                return Err(MzZipError::FileWriteFailed);
            }

            current_ofs += to_write as u64;
            remaining -= to_write;
        }

        Ok(())
    }

    pub fn mz_zip_writer_add_mem_ex(
        &mut self,
        archive_name: &str,
        buf: &[u8],
        comment: &[u8],
        comment_size: u16,
        level_and_flags: u32,
        uncomp_size: u64,
        uncomp_crc32: u32,
    ) -> Result<(), MzZipError> {
        self.mz_zip_writer_add_mem_ex_v2(
            archive_name,
            buf,
            if comment.is_empty() { None } else { Some(comment) },
            level_and_flags,
            uncomp_size,
            uncomp_crc32,
            None,
            None,
            None,
        )
    }
}

impl MzZipArchive {
    pub fn mz_zip_writer_add_mem_ex_v2(
        &mut self,
        archive_name: &str,
        buf: &[u8],
        comment: Option<&[u8]>,
        level_and_flags: u32,
        uncomp_size: u64,
        uncomp_crc32: u32,
        last_modified: Option<SystemTime>,
        user_extra_data: Option<&[u8]>,
        user_extra_data_central: Option<&[u8]>,
    ) -> Result<(), MzZipError> {
        let mut method = 0u16;
        let mut dos_time = 0u16;
        let mut dos_date = 0u16;
        let level = level_and_flags & 0xF;
        let store_data_uncompressed = level == 0 || (level_and_flags & MZ_ZIP_FLAG_COMPRESSED_DATA) != 0;
        let mut ext_attributes = 0u32;
        let mut bit_flags = 0u16;
        let mut extra_data = [0u8; MZ_ZIP64_MAX_CENTRAL_EXTRA_FIELD_SIZE as usize];
        let mut extra_size = 0u32;
        
        if (level_and_flags as i32) < 0 {
            // level_and_flags parameter is negative, use default level
            // (The C code uses MZ_DEFAULT_LEVEL which we need to define)
            // For now, we'll assume it means level 0
        }

        if uncomp_size > 0 || (!buf.is_empty() && (level_and_flags & MZ_ZIP_FLAG_COMPRESSED_DATA) == 0) {
            bit_flags |= MZ_ZIP_LDH_BIT_FLAG_HAS_LOCATOR as u16;
        }

        if (level_and_flags & MZ_ZIP_FLAG_ASCII_FILENAME) == 0 {
            bit_flags |= MZ_ZIP_GENERAL_PURPOSE_BIT_FLAG_UTF8 as u16;
        }

        // Parameter validation
        if self.m_zip_mode != MzZipMode::Writing {
            mz_zip_set_error(self,MzZipError::InvalidParameter);
            return Err(MzZipError::InvalidParameter);
        }
        
        if buf.is_empty() && uncomp_size == 0 && archive_name.ends_with('/') {
            // Directory entry - this is valid
        } else if buf.is_empty() && uncomp_size > 0 {
            mz_zip_set_error(self,MzZipError::InvalidParameter);
            return Err(MzZipError::InvalidParameter);
        }
        
        if comment.is_some() && comment.unwrap().is_empty() {
            mz_zip_set_error(self,MzZipError::InvalidParameter);
            return Err(MzZipError::InvalidParameter);
        }

        let archive_name_bytes = archive_name.as_bytes();
        let archive_name_size = archive_name_bytes.len();
        if archive_name_size > MZ_UINT16_MAX as usize {
            mz_zip_set_error(self,MzZipError::InvalidFilename);
            return Err(MzZipError::InvalidFilename);
        }

        let comment_size = comment.map_or(0, |c| c.len());
        if comment_size > MZ_UINT16_MAX as usize {
            mz_zip_set_error(self,MzZipError::InvalidParameter);
            return Err(MzZipError::InvalidParameter);
        }

        let user_extra_data_len = user_extra_data.map_or(0, |d| d.len());
        let user_extra_data_central_len = user_extra_data_central.map_or(0, |d| d.len());

        // Scoped state borrow: check zip64, update flag, extract as local bool
        let is_zip64 = {
            let state = match &mut self.m_pstate {
                Some(state) => state,
                None => {
                    self.m_last_error = MzZipError::InvalidParameter;
                    return Err(MzZipError::InvalidParameter);
                }
            };
            let requires_zip64 = state.zip64
                || self.m_total_files == MZ_UINT32_MAX as u32
                || buf.len() as u64 > 0xFFFFFFFF
                || uncomp_size > 0xFFFFFFFF
                || self.m_archive_size > 0xFFFFFFFF;
            if requires_zip64 {
                state.zip64 = true;
            }
            state.zip64
        };

        // Validate archive name
        if !mz_zip_writer_validate_archive_name(archive_name) {
            self.m_last_error = MzZipError::InvalidFilename;
            return Err(MzZipError::InvalidFilename);
        }

        // Handle time
        let now = SystemTime::now();
        let timestamp = last_modified.unwrap_or(now);
        let (d_time, d_date) = Self::mz_zip_time_t_to_dos_time(timestamp);
        dos_time = d_time;
        dos_date = d_date;

        // Calculate actual uncompressed size and CRC if not provided
        let (actual_uncomp_size, actual_uncomp_crc32) = if (level_and_flags & MZ_ZIP_FLAG_COMPRESSED_DATA) == 0 {
            let crc = mz_crc32(MZ_CRC32_INIT, buf);
            (buf.len() as u64, crc)
        } else {
            (uncomp_size, uncomp_crc32)
        };

        // Set compression method
        if !store_data_uncompressed || (level_and_flags & MZ_ZIP_FLAG_COMPRESSED_DATA) != 0 {
            method = MZ_DEFLATED as u16;
        }

        // Handle zip64 extra data
        let mut p_extra_data_vec: Vec<u8> = Vec::new();
        if is_zip64 {
            let archive_size = self.m_archive_size;
            if actual_uncomp_size >= MZ_UINT32_MAX as u64 || archive_size >= MZ_UINT32_MAX as u64 {
                extra_size = mz_zip_writer_create_zip64_extra_data(
                    &mut extra_data,
                    if actual_uncomp_size >= MZ_UINT32_MAX as u64 { Some(&actual_uncomp_size) } else { None },
                    None,
                    if archive_size >= MZ_UINT32_MAX as u64 { Some(&archive_size) } else { None },
                ) as u32;
                p_extra_data_vec = extra_data[..extra_size as usize].to_vec();
            }
        }

        // Check for directory
        if archive_name.ends_with('/') {
            ext_attributes |= MZ_ZIP_DOS_DIR_ATTRIBUTE_BITFLAG as u32;
            if !buf.is_empty() || actual_uncomp_size > 0 {
                self.m_last_error = MzZipError::InvalidParameter;
                return Err(MzZipError::InvalidParameter);
            }
        }

        // Calculate alignment padding
        let num_alignment_padding_bytes = self.mz_zip_writer_compute_padding_needed_for_file_alignment();

        // Reserve space in central directory
        let _central_dir_entry_size = MZ_ZIP_CENTRAL_DIR_HEADER_SIZE as usize
            + archive_name_size
            + comment_size
            + (if is_zip64 { MZ_ZIP64_MAX_CENTRAL_EXTRA_FIELD_SIZE as usize } else { 0 });

        // Write alignment zeros
        if num_alignment_padding_bytes > 0 {
            let ofs = self.m_archive_size;
            self.mz_zip_writer_write_zeros(ofs, num_alignment_padding_bytes)?;
            self.m_archive_size += num_alignment_padding_bytes as u64;
        }

        let local_dir_header_ofs = self.m_archive_size;

        // Create and write local directory header
        let mut local_dir_header = [0u8; MZ_ZIP_LOCAL_DIR_HEADER_SIZE as usize];

        let extra_field_size = (p_extra_data_vec.len() + user_extra_data_len) as u16;

        if !mz_zip_writer_create_local_dir_header(
            self,
            &mut local_dir_header,
            archive_name_size as u16,
            extra_field_size,
            actual_uncomp_size,
            0,
            actual_uncomp_crc32,
            method,
            bit_flags,
            dos_time,
            dos_date,
        ) {
            self.m_last_error = MzZipError::InternalError;
            return Err(MzZipError::InternalError);
        }

        // Write local header
        let written = self.write_data(local_dir_header_ofs, &local_dir_header)
            .map_err(|_| MzZipError::FileWriteFailed)?;
        if written != local_dir_header.len() {
            self.m_last_error = MzZipError::FileWriteFailed;
            return Err(MzZipError::FileWriteFailed);
        }
        self.m_archive_size += written as u64;

        // Write archive name
        let ofs = self.m_archive_size;
        let written = self.write_data(ofs, archive_name_bytes)
            .map_err(|_| MzZipError::FileWriteFailed)?;
        if written != archive_name_size {
            self.m_last_error = MzZipError::FileWriteFailed;
            return Err(MzZipError::FileWriteFailed);
        }
        self.m_archive_size += written as u64;

        // Write extra data if present
        if !p_extra_data_vec.is_empty() {
            let ofs = self.m_archive_size;
            let extra_copy = p_extra_data_vec.clone();
            let written = self.write_data(ofs, &extra_copy)
                .map_err(|_| MzZipError::FileWriteFailed)?;
            if written != extra_copy.len() {
                self.m_last_error = MzZipError::FileWriteFailed;
                return Err(MzZipError::FileWriteFailed);
            }
            self.m_archive_size += written as u64;
        }

        // Write user extra data if present
        if let Some(user_data) = user_extra_data {
            let ofs = self.m_archive_size;
            let written = self.write_data(ofs, user_data)
                .map_err(|_| MzZipError::FileWriteFailed)?;
            if written != user_data.len() {
                self.m_last_error = MzZipError::FileWriteFailed;
                return Err(MzZipError::FileWriteFailed);
            }
            self.m_archive_size += written as u64;
        }

        // Compress or store the data
        let comp_size = if store_data_uncompressed {
            let ofs = self.m_archive_size;
            let written = self.write_data(ofs, buf)
                .map_err(|_| MzZipError::FileWriteFailed)?;
            if written != buf.len() {
                self.m_last_error = MzZipError::FileWriteFailed;
                return Err(MzZipError::FileWriteFailed);
            }
            self.m_archive_size += written as u64;
            buf.len() as u64
        } else if !buf.is_empty() {
            // Compress with DEFLATE using tdef compressor
            let comp_flags = crate::tdef::tdefl_create_comp_flags_from_zip_params(
                level as i32, -1, 0,
            );
            let compressed_data = crate::tdef::tdefl_compress_mem_to_vec(buf, comp_flags)
                .ok_or(MzZipError::CompressionFailed)?;
            let ofs = self.m_archive_size;
            let written = self.write_data(ofs, &compressed_data)
                .map_err(|_| MzZipError::CompressionFailed)?;
            self.m_archive_size += written as u64;
            written as u64
        } else {
            0u64
        };

        // Write data descriptor if needed
        if (bit_flags & MZ_ZIP_LDH_BIT_FLAG_HAS_LOCATOR as u16) != 0 {
            let mut local_dir_footer = [0u8; 24];
            let footer_size = if is_zip64 { 24 } else { 16 };

            write_le32(&mut local_dir_footer, 0, MZ_ZIP_DATA_DESCRIPTOR_ID as u32);
            write_le32(&mut local_dir_footer, 4, actual_uncomp_crc32);

            if is_zip64 {
                write_le64(&mut local_dir_footer, 8, comp_size);
                write_le64(&mut local_dir_footer, 16, actual_uncomp_size);
            } else {
                write_le32(&mut local_dir_footer, 8, comp_size as u32);
                write_le32(&mut local_dir_footer, 12, actual_uncomp_size as u32);
            }

            let ofs = self.m_archive_size;
            let written = self.write_data(ofs, &local_dir_footer[..footer_size])
                .map_err(|_| MzZipError::FileWriteFailed)?;
            if written != footer_size {
                self.m_last_error = MzZipError::FileWriteFailed;
                return Err(MzZipError::FileWriteFailed);
            }
            self.m_archive_size += written as u64;
        }

        // Update zip64 extra data with actual compressed size if needed
        if is_zip64 && (actual_uncomp_size >= MZ_UINT32_MAX as u64 || comp_size >= MZ_UINT32_MAX as u64) {
            extra_size = mz_zip_writer_create_zip64_extra_data(
                &mut extra_data,
                if actual_uncomp_size >= MZ_UINT32_MAX as u64 { Some(&actual_uncomp_size) } else { None },
                if comp_size >= MZ_UINT32_MAX as u64 { Some(&comp_size) } else { None },
                if local_dir_header_ofs >= MZ_UINT32_MAX as u64 { Some(&local_dir_header_ofs) } else { None },
            ) as u32;
            p_extra_data_vec = extra_data[..extra_size as usize].to_vec();
        }

        // Add to central directory
        self.mz_zip_writer_add_to_central_dir(
            archive_name,
            archive_name_size as u16,
            if p_extra_data_vec.is_empty() { &[] } else { &p_extra_data_vec },
            extra_size as u16,
            comment.unwrap_or(&[]),
            comment_size as u16,
            actual_uncomp_size,
            comp_size,
            actual_uncomp_crc32,
            method,
            bit_flags,
            dos_time,
            dos_date,
            local_dir_header_ofs,
            ext_attributes,
            user_extra_data_central.unwrap_or(&[]),
            user_extra_data_central_len as u16,
        )?;

        self.m_total_files += 1;
        Ok(())
    }

    // Helper function to convert SystemTime to DOS time
    fn mz_zip_time_t_to_dos_time(timestamp: SystemTime) -> (u16, u16) {
        // This is a simplified version - actual implementation needs to handle
        // the conversion from SystemTime to DOS date/time format
        let duration = timestamp.duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0));
        let secs = duration.as_secs();
        
        // DOS time format:
        // bits 0-4: second/2 (0-29)
        // bits 5-10: minute (0-59)
        // bits 11-15: hour (0-23)
        let seconds = (secs % 60) as u16;
        let minutes = ((secs / 60) % 60) as u16;
        let hours = ((secs / 3600) % 24) as u16;
        let dos_time = (hours << 11) | (minutes << 5) | (seconds / 2);
        
        // DOS date format:
        // bits 0-4: day (1-31)
        // bits 5-8: month (1-12)
        // bits 9-15: year-1980 (0-127)
        // Using a fixed date for simplicity
        let dos_date = (1 << 5) | 1; // Jan 1
        
        (dos_time, dos_date)
    }
}

// Constants needed from the C code
const MZ_DEFAULT_LEVEL: u32 = 6;
const MZ_ZIP_FLAG_COMPRESSED_DATA: u32 = 0x40000;
const MZ_ZIP_FLAG_ASCII_FILENAME: u32 = 0x80000;
const MZ_UBER_COMPRESSION: u32 = 10;

// --- Module: mz_p7 ---

// --- Module: mz_p8 ---

// --- Module: mz_p9 ---

pub fn mz_zip_get_mode(pZip: Option<&MzZipArchive>) -> MzZipMode {
    pZip.map_or(MzZipMode::Invalid, |zip| zip.m_zip_mode)
}

pub fn mz_zip_get_type(pZip: Option<&MzZipArchive>) -> MzZipType {
    pZip.map_or(MzZipType::Invalid, |zip| zip.m_zip_type)
}

pub fn mz_zip_set_last_error(pZip: &mut MzZipArchive, err_num: MzZipError) -> MzZipError {
    let prev_err = pZip.m_last_error;
    pZip.m_last_error = err_num;
    prev_err
}

pub fn mz_zip_peek_last_error(pZip: Option<&MzZipArchive>) -> MzZipError {
    pZip.map_or(MzZipError::InvalidParameter, |zip| zip.m_last_error)
}

pub fn mz_zip_clear_last_error(pZip: &mut MzZipArchive) -> MzZipError {
    mz_zip_set_last_error(pZip, MzZipError::NoError)
}

pub fn mz_zip_get_last_error(pZip: &mut MzZipArchive) -> MzZipError {
    let prev_err = pZip.m_last_error;
    pZip.m_last_error = MzZipError::NoError;
    prev_err
}

pub fn mz_zip_get_error_string(mz_err: MzZipError) -> &'static str {
    match mz_err {
        MzZipError::NoError => "no error",
        MzZipError::UndefinedError => "undefined error",
        MzZipError::TooManyFiles => "too many files",
        MzZipError::FileTooLarge => "file too large",
        MzZipError::UnsupportedMethod => "unsupported method",
        MzZipError::UnsupportedEncryption => "unsupported encryption",
        MzZipError::UnsupportedFeature => "unsupported feature",
        MzZipError::FailedFindingCentralDir => "failed finding central directory",
        MzZipError::NotAnArchive => "not a ZIP archive",
        MzZipError::InvalidHeaderOrCorrupted => "invalid header or archive is corrupted",
        MzZipError::UnsupportedMultidisk => "unsupported multidisk archive",
        MzZipError::DecompressionFailed => "decompression failed or archive is corrupted",
        MzZipError::CompressionFailed => "compression failed",
        MzZipError::UnexpectedDecompressedSize => "unexpected decompressed size",
        MzZipError::CrcCheckFailed => "CRC-32 check failed",
        MzZipError::UnsupportedCdirSize => "unsupported central directory size",
        MzZipError::AllocFailed => "allocation failed",
        MzZipError::FileOpenFailed => "file open failed",
        MzZipError::FileCreateFailed => "file create failed",
        MzZipError::FileWriteFailed => "file write failed",
        MzZipError::FileReadFailed => "file read failed",
        MzZipError::FileCloseFailed => "file close failed",
        MzZipError::FileSeekFailed => "file seek failed",
        MzZipError::FileStatFailed => "file stat failed",
        MzZipError::InvalidParameter => "invalid parameter",
        MzZipError::InvalidFilename => "invalid filename",
        MzZipError::BufTooSmall => "buffer too small",
        MzZipError::InternalError => "internal error",
        MzZipError::FileNotFound => "file not found",
        MzZipError::ArchiveTooLarge => "archive is too large",
        MzZipError::ValidationFailed => "validation failed",
        MzZipError::WriteCallbackFailed => "write callback failed",
        MzZipError::FileTooBig => "file too big",
        MzZipError::UnexpectedDecompSize => "unexpected decompressed size",
        MzZipError::TotalErrors => "total errors",
    }
}

pub fn mz_zip_is_zip64(pZip: Option<&MzZipArchive>) -> bool {
    pZip.and_then(|zip| zip.m_pstate.as_ref())
        .map_or(false, |state| state.zip64)
}

pub fn mz_zip_get_central_dir_size(pZip: Option<&MzZipArchive>) -> usize {
    pZip.and_then(|zip| zip.m_pstate.as_ref())
        .map_or(0, |state| state.central_dir.size)
}

pub fn mz_zip_reader_get_num_files(pZip: Option<&MzZipArchive>) -> u32 {
    pZip.map_or(0, |zip| zip.m_total_files)
}

pub fn mz_zip_get_archive_size(pZip: Option<&MzZipArchive>) -> u64 {
    pZip.map_or(0, |zip| zip.m_archive_size)
}

pub fn mz_zip_get_archive_file_start_offset(pZip: Option<&MzZipArchive>) -> u64 {
    pZip.and_then(|zip| zip.m_pstate.as_ref())
        .map_or(0, |state| state.file_archive_start_ofs)
}

// Note: MZ_FILE is typically std::fs::File in Rust
pub fn mz_zip_get_cfile(pZip: Option<&MzZipArchive>) -> Option<&std::fs::File> {
    pZip.and_then(|zip| zip.m_pstate.as_ref())
        .and_then(|state| state.pfile.as_ref())
}

pub fn mz_zip_read_archive_data(
    pZip: &mut MzZipArchive,
    file_ofs: u64,
    pBuf: &mut [u8],
) -> Result<usize, MzZipError> {
    if pZip.m_pstate.is_none() || pBuf.is_empty() || pZip.m_pread.is_none() {
        mz_zip_set_error(pZip, MzZipError::InvalidParameter);
        return Err(MzZipError::InvalidParameter);
    }

    // Copy function pointer to avoid borrow conflict
    let read_fn = pZip.m_pread.ok_or(MzZipError::InvalidParameter)?;
    let n = read_fn(pZip, file_ofs, pBuf);
    Ok(n)
}

pub fn mz_zip_reader_get_filename(
    pZip: &mut MzZipArchive,
    file_index: u32,
    pFilename: &mut [u8],
) -> Result<u32, MzZipError> {
    // Get central directory header
    let cdh = mz_zip_get_cdh(pZip, file_index);
    if cdh.is_none() {
        mz_zip_set_error(pZip, MzZipError::InvalidParameter);
        if !pFilename.is_empty() {
            pFilename[0] = 0;
        }
        return Ok(0);
    }

    let cdh_slice = cdh.unwrap();

    // Read filename length from header
    if cdh_slice.len() < MZ_ZIP_CDH_FILENAME_LEN_OFS as usize + 2 {
        mz_zip_set_error(pZip, MzZipError::InvalidHeaderOrCorrupted);
        return Ok(0);
    }
    
    let filename_len = u16::from_le_bytes([
        cdh_slice[MZ_ZIP_CDH_FILENAME_LEN_OFS as usize],
        cdh_slice[MZ_ZIP_CDH_FILENAME_LEN_OFS as usize + 1],
    ]) as usize;
    
    let filename_start = MZ_ZIP_CENTRAL_DIR_HEADER_SIZE as usize;
    
    // Copy filename to buffer if buffer is provided
    if !pFilename.is_empty() {
        let copy_len = filename_len.min(pFilename.len() - 1);
        
        if cdh_slice.len() >= filename_start + copy_len {
            pFilename[..copy_len].copy_from_slice(
                &cdh_slice[filename_start..filename_start + copy_len],
            );
        }
        pFilename[copy_len] = 0;
    }
    
    // Return required buffer size (including null terminator)
    Ok((filename_len + 1) as u32)
}

pub fn mz_zip_reader_file_stat(
    pZip: &MzZipArchive,
    file_index: u32,
    pStat: &mut MzZipArchiveFileStat,
) -> bool {
    let cdh = mz_zip_get_cdh(pZip, file_index);
    mz_zip_file_stat_internal(pZip, file_index, cdh, pStat)
}

pub fn mz_zip_end(pZip: &mut MzZipArchive) -> bool {
    match pZip.m_zip_mode {
        MzZipMode::Reading => mz_zip_reader_end(pZip),
        MzZipMode::Writing | MzZipMode::WritingHasBeenFinalized => {
            #[cfg(feature = "archive_writing")]
            {
                mz_zip_writer_end(pZip)
            }
            #[cfg(not(feature = "archive_writing"))]
            {
                false
            }
        }
        _ => false,
    }
}

// --- Helper: read LE values from byte slices ---
fn read_le16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_le32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]])
}

fn read_le64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3],
        bytes[offset + 4], bytes[offset + 5], bytes[offset + 6], bytes[offset + 7],
    ])
}

fn mz_tolower(c: u8) -> u8 {
    if c >= b'A' && c <= b'Z' { c - b'A' + b'a' } else { c }
}

// --- mz_zip_get_cdh: get central directory header for file_index ---
fn mz_zip_get_cdh(pZip: &MzZipArchive, file_index: u32) -> Option<&[u8]> {
    let state = pZip.m_pstate.as_ref()?;
    if file_index >= pZip.m_total_files {
        return None;
    }

    let central_dir = state.central_dir.as_bytes()?;
    let offsets = state.central_dir_offsets.as_bytes()?;

    let idx = file_index as usize;
    if idx * 4 + 4 > offsets.len() {
        return None;
    }

    let offset = u32::from_le_bytes([
        offsets[idx * 4], offsets[idx * 4 + 1], offsets[idx * 4 + 2], offsets[idx * 4 + 3],
    ]) as usize;

    if offset >= central_dir.len() {
        return None;
    }

    // Determine end: next file's offset or end of central_dir
    let next_offset = if (idx + 1) < pZip.m_total_files as usize && (idx + 1) * 4 + 4 <= offsets.len() {
        u32::from_le_bytes([
            offsets[(idx + 1) * 4], offsets[(idx + 1) * 4 + 1],
            offsets[(idx + 1) * 4 + 2], offsets[(idx + 1) * 4 + 3],
        ]) as usize
    } else {
        central_dir.len()
    };

    if offset < next_offset && next_offset <= central_dir.len() {
        Some(&central_dir[offset..next_offset])
    } else {
        Some(&central_dir[offset..])
    }
}

// --- mz_zip_file_stat_internal: parse CDH into MzZipArchiveFileStat ---
fn mz_zip_file_stat_internal(
    pZip: &MzZipArchive,
    file_index: u32,
    cdh: Option<&[u8]>,
    pStat: &mut MzZipArchiveFileStat,
) -> bool {
    let p = match cdh {
        Some(data) if data.len() >= MZ_ZIP_CENTRAL_DIR_HEADER_SIZE => data,
        _ => return false,
    };

    let state = match pZip.m_pstate.as_ref() {
        Some(s) => s,
        None => return false,
    };

    // Get offset from central_dir_offsets
    let offsets = match state.central_dir_offsets.as_bytes() {
        Some(o) => o,
        None => return false,
    };
    let offset_idx = file_index as usize * 4;
    let central_dir_ofs = if offset_idx + 4 <= offsets.len() {
        u32::from_le_bytes([offsets[offset_idx], offsets[offset_idx + 1], offsets[offset_idx + 2], offsets[offset_idx + 3]]) as u64
    } else {
        0u64
    };

    pStat.m_file_index = file_index;
    pStat.m_central_dir_ofs = central_dir_ofs;
    pStat.m_version_made_by = read_le16(p, MZ_ZIP_CDH_VERSION_MADE_BY_OFS);
    pStat.m_version_needed = read_le16(p, MZ_ZIP_CDH_VERSION_NEEDED_OFS);
    pStat.m_bit_flag = read_le16(p, MZ_ZIP_CDH_BIT_FLAG_OFS);
    pStat.m_method = read_le16(p, MZ_ZIP_CDH_METHOD_OFS);
    pStat.m_crc32 = read_le32(p, MZ_ZIP_CDH_CRC32_OFS);
    pStat.m_comp_size = read_le32(p, MZ_ZIP_CDH_COMPRESSED_SIZE_OFS) as u64;
    pStat.m_uncomp_size = read_le32(p, MZ_ZIP_CDH_DECOMPRESSED_SIZE_OFS) as u64;
    pStat.m_internal_attr = read_le16(p, MZ_ZIP_CDH_INTERNAL_ATTR_OFS);
    pStat.m_external_attr = read_le32(p, MZ_ZIP_CDH_EXTERNAL_ATTR_OFS);
    pStat.m_local_header_ofs = read_le32(p, MZ_ZIP_CDH_LOCAL_HEADER_OFS) as u64;

    // DOS time -> simple timestamp
    let file_time = read_le16(p, MZ_ZIP_CDH_FILE_TIME_OFS);
    let file_date = read_le16(p, MZ_ZIP_CDH_FILE_DATE_OFS);
    pStat.m_time = ((file_date as u64) << 16) | (file_time as u64);

    // Extract filename
    let filename_len = read_le16(p, MZ_ZIP_CDH_FILENAME_LEN_OFS) as usize;
    let filename_start = MZ_ZIP_CENTRAL_DIR_HEADER_SIZE;
    let filename_end = filename_start + filename_len;
    if filename_end <= p.len() {
        pStat.m_filename = String::from_utf8_lossy(&p[filename_start..filename_end]).into_owned();
    } else {
        pStat.m_filename = String::new();
    }

    // Extract comment
    let extra_len = read_le16(p, MZ_ZIP_CDH_EXTRA_LEN_OFS) as usize;
    let comment_len = read_le16(p, MZ_ZIP_CDH_COMMENT_LEN_OFS) as usize;
    let comment_start = filename_start + filename_len + extra_len;
    let comment_end = comment_start + comment_len;
    if comment_end <= p.len() {
        pStat.m_comment = String::from_utf8_lossy(&p[comment_start..comment_end]).into_owned();
        pStat.m_comment_size = comment_len as u32;
    } else {
        pStat.m_comment = String::new();
        pStat.m_comment_size = 0;
    }

    // Directory check
    pStat.m_is_directory = (filename_len > 0 && pStat.m_filename.ends_with('/'))
        || (pStat.m_external_attr & MZ_ZIP_DOS_DIR_ATTRIBUTE_BITFLAG as u32) != 0;

    // Encryption check
    pStat.m_is_encrypted = (pStat.m_bit_flag & MZ_ZIP_GENERAL_PURPOSE_BIT_FLAG_IS_ENCRYPTED as u16) != 0;

    // Support check
    pStat.m_is_supported = !pStat.m_is_encrypted && (pStat.m_method == 0 || pStat.m_method == MZ_DEFLATED as u16);

    // Check for zip64 extended information
    if pStat.m_comp_size == 0xFFFFFFFF || pStat.m_uncomp_size == 0xFFFFFFFF || pStat.m_local_header_ofs == 0xFFFFFFFF {
        let extra_start = filename_start + filename_len;
        if extra_len > 0 && extra_start + extra_len <= p.len() {
            let mut extra_remaining = extra_len;
            let mut extra_pos = extra_start;

            while extra_remaining >= 4 {
                let field_id = read_le16(p, extra_pos);
                let field_data_size = read_le16(p, extra_pos + 2) as usize;

                if field_data_size + 4 > extra_remaining {
                    break;
                }

                if field_id == MZ_ZIP64_EXTENDED_INFORMATION_FIELD_HEADER_ID as u16 {
                    let mut field_data_pos = extra_pos + 4;
                    let mut field_remaining = field_data_size;

                    if pStat.m_uncomp_size == 0xFFFFFFFF && field_remaining >= 8 {
                        pStat.m_uncomp_size = read_le64(p, field_data_pos);
                        field_data_pos += 8;
                        field_remaining -= 8;
                    }

                    if pStat.m_comp_size == 0xFFFFFFFF && field_remaining >= 8 {
                        pStat.m_comp_size = read_le64(p, field_data_pos);
                        field_data_pos += 8;
                        field_remaining -= 8;
                    }

                    if pStat.m_local_header_ofs == 0xFFFFFFFF && field_remaining >= 8 {
                        pStat.m_local_header_ofs = read_le64(p, field_data_pos);
                    }

                    break;
                }

                let skip = 4 + field_data_size;
                extra_pos += skip;
                extra_remaining -= skip;
            }
        }
    }

    true
}

// --- mz_zip_reader_locate_file_v2: search central directory for a file by name ---
fn mz_zip_reader_locate_file_v2(
    pZip: &MzZipArchive,
    pArchive_name: &str,
    pComment: Option<&str>,
    flags: u32,
) -> Option<u32> {
    if pZip.m_pstate.is_none() || pArchive_name.is_empty() {
        return None;
    }

    let state = pZip.m_pstate.as_ref()?;
    let central_dir = state.central_dir.as_bytes()?;
    let offsets = state.central_dir_offsets.as_bytes()?;
    let name_len = pArchive_name.len();
    let name_bytes = pArchive_name.as_bytes();

    for file_index in 0..pZip.m_total_files {
        let idx = file_index as usize;
        if idx * 4 + 4 > offsets.len() {
            continue;
        }

        let offset = u32::from_le_bytes([
            offsets[idx * 4], offsets[idx * 4 + 1], offsets[idx * 4 + 2], offsets[idx * 4 + 3],
        ]) as usize;

        if offset >= central_dir.len() || offset + MZ_ZIP_CENTRAL_DIR_HEADER_SIZE > central_dir.len() {
            continue;
        }

        let p_header = &central_dir[offset..];
        let filename_len = read_le16(p_header, MZ_ZIP_CDH_FILENAME_LEN_OFS) as usize;

        if filename_len < name_len {
            continue;
        }

        // Check comment if provided
        if let Some(comment) = pComment {
            let file_extra_len = read_le16(p_header, MZ_ZIP_CDH_EXTRA_LEN_OFS) as usize;
            let file_comment_len = read_le16(p_header, MZ_ZIP_CDH_COMMENT_LEN_OFS) as usize;

            if file_comment_len != comment.len() {
                continue;
            }

            let comment_start = MZ_ZIP_CENTRAL_DIR_HEADER_SIZE + filename_len + file_extra_len;
            let comment_end = comment_start + file_comment_len;

            if comment_end > p_header.len() {
                continue;
            }

            let file_comment_bytes = &p_header[comment_start..comment_end];
            if file_comment_bytes != comment.as_bytes() {
                continue;
            }
        }

        // Compare filenames (case-insensitive by default)
        if filename_len == name_len {
            let filename_start = MZ_ZIP_CENTRAL_DIR_HEADER_SIZE;
            if filename_start + filename_len <= p_header.len() {
                let filename_bytes = &p_header[filename_start..filename_start + filename_len];
                let matches = filename_bytes.iter().zip(name_bytes.iter())
                    .all(|(&a, &b)| mz_tolower(a) == mz_tolower(b));
                if matches {
                    return Some(file_index);
                }
            }
        }
    }

    None
}

// --- mz_zip_reader_extract_to_heap: extract file to a heap-allocated Vec<u8> ---
fn mz_zip_reader_extract_to_heap(
    pZip: &MzZipArchive,
    file_index: u32,
    flags: u32,
) -> Result<Vec<u8>, MzZipError> {
    // Get file stats
    let cdh = mz_zip_get_cdh(pZip, file_index).ok_or(MzZipError::InvalidParameter)?;
    let mut file_stat = MzZipArchiveFileStat {
        m_file_index: 0, m_central_dir_ofs: 0, m_version_made_by: 0,
        m_version_needed: 0, m_bit_flag: 0, m_method: 0, m_crc32: 0,
        m_comp_size: 0, m_uncomp_size: 0, m_internal_attr: 0,
        m_external_attr: 0, m_local_header_ofs: 0, m_comment_size: 0,
        m_is_directory: false, m_is_encrypted: false, m_is_supported: false,
        m_filename: String::new(), m_comment: String::new(), m_time: 0,
    };

    if !mz_zip_file_stat_internal(pZip, file_index, Some(cdh), &mut file_stat) {
        return Err(MzZipError::FileNotFound);
    }

    // Directory or zero length
    if file_stat.m_is_directory || file_stat.m_uncomp_size == 0 {
        return Ok(Vec::new());
    }

    // Encryption not supported
    if file_stat.m_is_encrypted {
        return Err(MzZipError::UnsupportedEncryption);
    }

    // Only store (0) and deflate (8) supported
    if file_stat.m_method != 0 && file_stat.m_method != MZ_DEFLATED as u16 {
        return Err(MzZipError::UnsupportedMethod);
    }

    let alloc_size = file_stat.m_uncomp_size as usize;
    let mut buf = vec![0u8; alloc_size];

    // Read local header to find data offset
    let state = pZip.m_pstate.as_ref().ok_or(MzZipError::InternalError)?;
    let mut local_header = [0u8; MZ_ZIP_LOCAL_DIR_HEADER_SIZE];
    // Need mutable borrow for read_data - but we only have immutable pZip.
    // Use the internal state directly.
    let state_ptr = pZip.m_pstate.as_ref().ok_or(MzZipError::InternalError)?;

    // Read from memory if available
    if let Some(ref pmem) = state_ptr.pmem {
        let start = (state_ptr.file_archive_start_ofs + file_stat.m_local_header_ofs) as usize;
        if start + MZ_ZIP_LOCAL_DIR_HEADER_SIZE > pmem.len() {
            return Err(MzZipError::FileReadFailed);
        }
        local_header.copy_from_slice(&pmem[start..start + MZ_ZIP_LOCAL_DIR_HEADER_SIZE]);

        // Check signature
        let sig = read_le32(&local_header, 0);
        if sig != MZ_ZIP_LOCAL_DIR_HEADER_SIG as u32 {
            return Err(MzZipError::InvalidHeaderOrCorrupted);
        }

        let fname_len = read_le16(&local_header, MZ_ZIP_LDH_FILENAME_LEN_OFS) as u64;
        let extra_len = read_le16(&local_header, MZ_ZIP_LDH_EXTRA_LEN_OFS) as u64;
        let data_ofs = start as u64 + MZ_ZIP_LOCAL_DIR_HEADER_SIZE as u64 + fname_len + extra_len;

        if file_stat.m_method == 0 {
            // Stored: just copy
            let data_start = data_ofs as usize;
            let data_end = data_start + file_stat.m_uncomp_size as usize;
            if data_end > pmem.len() {
                return Err(MzZipError::FileReadFailed);
            }
            buf.copy_from_slice(&pmem[data_start..data_end]);

            // CRC check
            let crc = mz_crc32(MZ_CRC32_INIT, &buf);
            if crc != file_stat.m_crc32 {
                return Err(MzZipError::CrcCheckFailed);
            }
        } else {
            // Deflated: need decompression
            let comp_start = data_ofs as usize;
            let comp_end = comp_start + file_stat.m_comp_size as usize;
            if comp_end > pmem.len() {
                return Err(MzZipError::FileReadFailed);
            }
            let compressed = &pmem[comp_start..comp_end];
            let needed = file_stat.m_uncomp_size as usize;
            let decompressed = tinfl_decompress(compressed, needed)?;
            buf.copy_from_slice(&decompressed);

            // CRC check
            let crc = mz_crc32(MZ_CRC32_INIT, &buf);
            if crc != file_stat.m_crc32 {
                return Err(MzZipError::CrcCheckFailed);
            }
        }
    } else if let Some(read_fn) = pZip.m_pread {
        // File-based: read through the m_pread function pointer
        let file_start = pZip.m_pstate.as_ref()
            .map(|s| s.file_archive_start_ofs)
            .unwrap_or(0);
        let header_ofs = file_start + file_stat.m_local_header_ofs;

        // Read local header
        let n = read_fn(pZip, header_ofs, &mut local_header);
        if n != MZ_ZIP_LOCAL_DIR_HEADER_SIZE {
            return Err(MzZipError::FileReadFailed);
        }

        let sig = read_le32(&local_header, 0);
        if sig != MZ_ZIP_LOCAL_DIR_HEADER_SIG as u32 {
            return Err(MzZipError::InvalidHeaderOrCorrupted);
        }

        let fname_len = read_le16(&local_header, MZ_ZIP_LDH_FILENAME_LEN_OFS) as u64;
        let extra_len = read_le16(&local_header, MZ_ZIP_LDH_EXTRA_LEN_OFS) as u64;
        let data_ofs = header_ofs + MZ_ZIP_LOCAL_DIR_HEADER_SIZE as u64 + fname_len + extra_len;

        if file_stat.m_method == 0 {
            // Stored: read directly into output buffer
            let n = read_fn(pZip, data_ofs, &mut buf);
            if n != file_stat.m_uncomp_size as usize {
                return Err(MzZipError::FileReadFailed);
            }
            let crc = mz_crc32(MZ_CRC32_INIT, &buf);
            if crc != file_stat.m_crc32 {
                return Err(MzZipError::CrcCheckFailed);
            }
        } else {
            // Deflated: read compressed data, decompress
            let mut compressed = vec![0u8; file_stat.m_comp_size as usize];
            let n = read_fn(pZip, data_ofs, &mut compressed);
            if n != compressed.len() {
                return Err(MzZipError::FileReadFailed);
            }
            let needed = file_stat.m_uncomp_size as usize;
            let decompressed = tinfl_decompress(&compressed, needed)?;
            buf.copy_from_slice(&decompressed);
            let crc = mz_crc32(MZ_CRC32_INIT, &buf);
            if crc != file_stat.m_crc32 {
                return Err(MzZipError::CrcCheckFailed);
            }
        }
    } else {
        return Err(MzZipError::InvalidParameter);
    }

    Ok(buf)
}

// --- mz_zip_reader_end: clean up reader resources ---
fn mz_zip_reader_end(pZip: &mut MzZipArchive) -> bool {
    mz_zip_reader_end_internal(pZip, true);
    true
}

// --- mz_zip_reader_init_file_v2: init reader from open file handle ---
fn mz_zip_reader_init_file_v2(
    pZip: &mut MzZipArchive,
    file: &std::fs::File,
    flags: u32,
    file_start_ofs: u64,
    archive_size: u64,
) -> Result<(), MzZipError> {
    // Get file size
    let file_size = if archive_size > 0 {
        archive_size
    } else {
        file.metadata().map_err(|_| MzZipError::FileStatFailed)?.len()
    };

    if file_size < MZ_ZIP_END_OF_CENTRAL_DIR_HEADER_SIZE as u64 {
        return Err(MzZipError::NotAnArchive);
    }

    // Read entire file into memory for simplicity
    let mut file_clone = file.try_clone().map_err(|_| MzZipError::FileReadFailed)?;
    file_clone.seek(SeekFrom::Start(0)).map_err(|_| MzZipError::FileSeekFailed)?;
    let mut data = Vec::new();
    file_clone.read_to_end(&mut data).map_err(|_| MzZipError::FileReadFailed)?;

    // Init reader state
    pZip.m_zip_mode = MzZipMode::Invalid; // reset for init_reader
    pZip.init_reader()?;

    if let Some(state) = &mut pZip.m_pstate {
        state.pmem = Some(data);
        state.file_archive_start_ofs = file_start_ofs;
        state.init_flags = flags;
    }

    pZip.m_zip_type = MzZipType::File;
    pZip.m_archive_size = file_size - file_start_ofs;

    // Read central directory
    if !mz_zip_reader_read_central_dir(pZip, flags) {
        mz_zip_reader_end_internal(pZip, false);
        return Err(pZip.m_last_error);
    }

    Ok(())
}

// --- mz_zip_reader_end_internal: clean up internal state ---
fn mz_zip_reader_end_internal(pZip: &mut MzZipArchive, _extraction_succeeded: bool) {
    if let Some(mut state) = pZip.m_pstate.take() {
        state.central_dir.clear();
        state.central_dir_offsets.clear();
        state.sorted_central_dir_offsets.clear();
        state.pfile = None;
        state.pmem = None;
        // state is dropped here
    }
    pZip.m_zip_mode = MzZipMode::Invalid;
}

// --- mz_zip_writer_end: writer cleanup ---
#[cfg(feature = "archive_writing")]
fn mz_zip_writer_end(pZip: &mut MzZipArchive) -> bool {
    let mode = pZip.m_zip_mode;
    if mode != MzZipMode::Writing && mode != MzZipMode::WritingHasBeenFinalized {
        pZip.m_last_error = MzZipError::InvalidParameter;
        return false;
    }

    if let Some(mut state) = pZip.m_pstate.take() {
        state.central_dir.clear();
        state.central_dir_offsets.clear();
        state.sorted_central_dir_offsets.clear();
        state.pfile = None;
        state.pmem = None;
    } else {
        pZip.m_last_error = MzZipError::InvalidParameter;
        return false;
    }

    pZip.m_zip_mode = MzZipMode::Invalid;
    true
}

// --- mz_zip_reader_read_central_dir: read and parse central directory ---
fn mz_zip_reader_read_central_dir(pZip: &mut MzZipArchive, flags: u32) -> bool {
    let archive_size = pZip.m_archive_size;

    if archive_size < MZ_ZIP_END_OF_CENTRAL_DIR_HEADER_SIZE as u64 {
        pZip.m_last_error = MzZipError::NotAnArchive;
        return false;
    }

    // Phase 1: Parse EOCD from pmem (immutable borrow scope)
    // We extract all values we need, plus copy cdir_data, then drop the borrow.
    let parsed = {
        let state = match pZip.m_pstate.as_ref() {
            Some(s) => s,
            None => { pZip.m_last_error = MzZipError::InternalError; return false; }
        };

        let pmem = match state.pmem.as_ref() {
            Some(m) => m,
            None => { pZip.m_last_error = MzZipError::InternalError; return false; }
        };

        let start_ofs = state.file_archive_start_ofs as usize;
        if start_ofs >= pmem.len() { return false; }
        let archive_data = &pmem[start_ofs..];
        let archive_len = archive_data.len();

        if archive_len < MZ_ZIP_END_OF_CENTRAL_DIR_HEADER_SIZE {
            pZip.m_last_error = MzZipError::NotAnArchive;
            return false;
        }

        // Scan for EOCD
        let mut eocd_ofs_opt: Option<usize> = None;
        let search_start = if archive_len > 65535 + MZ_ZIP_END_OF_CENTRAL_DIR_HEADER_SIZE {
            archive_len - 65535 - MZ_ZIP_END_OF_CENTRAL_DIR_HEADER_SIZE
        } else {
            0
        };

        let mut i = archive_len - MZ_ZIP_END_OF_CENTRAL_DIR_HEADER_SIZE;
        loop {
            if read_le32(archive_data, i) == MZ_ZIP_END_OF_CENTRAL_DIR_HEADER_SIG as u32 {
                eocd_ofs_opt = Some(i);
                break;
            }
            if i == search_start { break; }
            i -= 1;
        }

        let eocd_ofs = match eocd_ofs_opt {
            Some(ofs) => ofs,
            None => { pZip.m_last_error = MzZipError::FailedFindingCentralDir; return false; }
        };

        let eocd = &archive_data[eocd_ofs..];

        let mut total_files = read_le16(eocd, MZ_ZIP_ECDH_CDIR_TOTAL_ENTRIES_OFS) as u32;
        let cdir_entries_on_disk = read_le16(eocd, MZ_ZIP_ECDH_CDIR_NUM_ENTRIES_ON_DISK_OFS) as u32;
        let num_this_disk = read_le16(eocd, MZ_ZIP_ECDH_NUM_THIS_DISK_OFS) as u32;
        let cdir_disk_index = read_le16(eocd, MZ_ZIP_ECDH_NUM_DISK_CDIR_OFS) as u32;
        let mut cdir_size = read_le32(eocd, MZ_ZIP_ECDH_CDIR_SIZE_OFS) as u64;
        let mut cdir_ofs = read_le32(eocd, MZ_ZIP_ECDH_CDIR_OFS_OFS) as u64;

        let mut is_zip64 = false;
        if eocd_ofs >= MZ_ZIP64_END_OF_CENTRAL_DIR_LOCATOR_SIZE + MZ_ZIP64_END_OF_CENTRAL_DIR_HEADER_SIZE {
            let locator_ofs = eocd_ofs - MZ_ZIP64_END_OF_CENTRAL_DIR_LOCATOR_SIZE;
            if read_le32(archive_data, locator_ofs) == MZ_ZIP64_END_OF_CENTRAL_DIR_LOCATOR_SIG as u32 {
                is_zip64 = true;
                let zip64_ecdr_ofs = read_le64(archive_data, locator_ofs + MZ_ZIP64_ECDL_REL_OFS_TO_ZIP64_ECDR_OFS);
                if (zip64_ecdr_ofs as usize) + MZ_ZIP64_END_OF_CENTRAL_DIR_HEADER_SIZE <= archive_len {
                    let z64 = &archive_data[zip64_ecdr_ofs as usize..];
                    if read_le32(z64, 0) == MZ_ZIP64_END_OF_CENTRAL_DIR_HEADER_SIG as u32 {
                        let z64_total = read_le64(z64, MZ_ZIP64_ECDH_CDIR_TOTAL_ENTRIES_OFS);
                        let z64_cdir_size = read_le64(z64, MZ_ZIP64_ECDH_CDIR_SIZE_OFS);
                        let z64_cdir_ofs = read_le64(z64, MZ_ZIP64_ECDH_CDIR_OFS_OFS);
                        if z64_total <= u32::MAX as u64 { total_files = z64_total as u32; }
                        cdir_size = z64_cdir_size;
                        cdir_ofs = z64_cdir_ofs;
                    }
                }
            }
        }

        // Validate
        if total_files != cdir_entries_on_disk && !is_zip64 {
            pZip.m_last_error = MzZipError::UnsupportedMultidisk;
            return false;
        }
        if ((num_this_disk | cdir_disk_index) != 0) && ((num_this_disk != 1) || (cdir_disk_index != 1)) {
            pZip.m_last_error = MzZipError::UnsupportedMultidisk;
            return false;
        }
        if cdir_ofs + cdir_size > archive_len as u64 {
            pZip.m_last_error = MzZipError::InvalidHeaderOrCorrupted;
            return false;
        }

        // Copy cdir bytes to owned Vec so we can drop the immutable borrow
        let cdir_bytes = if total_files > 0 {
            let cdir_start = cdir_ofs as usize;
            let cdir_end = cdir_start + cdir_size as usize;
            if cdir_end > archive_len {
                pZip.m_last_error = MzZipError::InvalidHeaderOrCorrupted;
                return false;
            }
            Some(archive_data[cdir_start..cdir_end].to_vec())
        } else {
            None
        };

        (total_files, cdir_ofs, cdir_size, is_zip64, cdir_bytes)
    };
    // Immutable borrow of pZip.m_pstate is now dropped.

    let (total_files, cdir_ofs, cdir_size, is_zip64, cdir_bytes_opt) = parsed;

    pZip.m_total_files = total_files;
    pZip.m_central_directory_file_ofs = cdir_ofs;

    // Phase 2: Store parsed data into state (mutable borrow)
    if total_files > 0 {
        let cdir_bytes = cdir_bytes_opt.unwrap();

        // Parse offsets by walking CDH entries
        let mut offsets_vec: Vec<u8> = Vec::with_capacity(total_files as usize * 4);
        let mut sorted_vec: Vec<u8> = Vec::with_capacity(total_files as usize * 4);
        let mut pos = 0usize;
        for i in 0..total_files {
            if pos + MZ_ZIP_CENTRAL_DIR_HEADER_SIZE > cdir_bytes.len() {
                pZip.m_last_error = MzZipError::InvalidHeaderOrCorrupted;
                return false;
            }
            let sig = read_le32(&cdir_bytes, pos);
            if sig != MZ_ZIP_CENTRAL_DIR_HEADER_SIG as u32 {
                pZip.m_last_error = MzZipError::InvalidHeaderOrCorrupted;
                return false;
            }
            offsets_vec.extend_from_slice(&(pos as u32).to_le_bytes());
            sorted_vec.extend_from_slice(&i.to_le_bytes());

            let filename_len = read_le16(&cdir_bytes, pos + MZ_ZIP_CDH_FILENAME_LEN_OFS) as usize;
            let extra_len = read_le16(&cdir_bytes, pos + MZ_ZIP_CDH_EXTRA_LEN_OFS) as usize;
            let comment_len = read_le16(&cdir_bytes, pos + MZ_ZIP_CDH_COMMENT_LEN_OFS) as usize;
            pos += MZ_ZIP_CENTRAL_DIR_HEADER_SIZE + filename_len + extra_len + comment_len;
        }

        let state = pZip.m_pstate.as_mut().unwrap();
        state.zip64 = is_zip64;

        state.central_dir.p = Some(cdir_bytes);
        state.central_dir.size = cdir_size as usize;
        state.central_dir.capacity = cdir_size as usize;
        state.central_dir.element_size = 1;

        state.central_dir_offsets.p = Some(offsets_vec);
        state.central_dir_offsets.size = total_files as usize;
        state.central_dir_offsets.capacity = total_files as usize;
        state.central_dir_offsets.element_size = 4;

        state.sorted_central_dir_offsets.p = Some(sorted_vec);
        state.sorted_central_dir_offsets.size = total_files as usize;
        state.sorted_central_dir_offsets.capacity = total_files as usize;
        state.sorted_central_dir_offsets.element_size = 4;
    } else if let Some(state) = pZip.m_pstate.as_mut() {
        state.zip64 = is_zip64;
    }

    true
}

// ============================================================================
// Additional API functions
// ============================================================================

// --- Constants for new functions ---
const MZ_ZIP_FLAG_CASE_SENSITIVE: u32 = 0x0100;
const MZ_ZIP_FLAG_IGNORE_PATH: u32 = 0x0200;
const MZ_ZIP_FLAG_VALIDATE_LOCATE_FILE_FLAG: u32 = 0x1000;
const MZ_ZIP_FLAG_VALIDATE_HEADERS_ONLY: u32 = 0x2000;
const MZ_ZIP_MAX_IO_BUF_SIZE: usize = 64 * 1024;

// --- Reader info functions ---

/// Check if a file entry in the archive is a directory.
pub fn mz_zip_reader_is_file_a_directory(
    p_zip: &MzZipArchive,
    file_index: u32,
) -> bool {
    let cdh = match mz_zip_get_cdh(p_zip, file_index) {
        Some(data) if data.len() >= MZ_ZIP_CENTRAL_DIR_HEADER_SIZE => data,
        _ => return false,
    };
    let filename_len = read_le16(cdh, MZ_ZIP_CDH_FILENAME_LEN_OFS) as usize;
    if filename_len > 0 {
        let fname_start = MZ_ZIP_CENTRAL_DIR_HEADER_SIZE;
        if fname_start + filename_len <= cdh.len() && cdh[fname_start + filename_len - 1] == b'/' {
            return true;
        }
    }
    let external_attr = read_le32(cdh, MZ_ZIP_CDH_EXTERNAL_ATTR_OFS);
    (external_attr & MZ_ZIP_DOS_DIR_ATTRIBUTE_BITFLAG as u32) != 0
}

/// Check if a file entry in the archive is encrypted.
pub fn mz_zip_reader_is_file_encrypted(
    p_zip: &MzZipArchive,
    file_index: u32,
) -> bool {
    let cdh = match mz_zip_get_cdh(p_zip, file_index) {
        Some(data) if data.len() >= MZ_ZIP_CENTRAL_DIR_HEADER_SIZE => data,
        _ => return false,
    };
    let bit_flag = read_le16(cdh, MZ_ZIP_CDH_BIT_FLAG_OFS);
    (bit_flag & (MZ_ZIP_GENERAL_PURPOSE_BIT_FLAG_IS_ENCRYPTED as u16)) != 0
}

/// Check if a file entry in the archive is supported (not encrypted, uses
/// store or deflate).
pub fn mz_zip_reader_is_file_supported(
    p_zip: &MzZipArchive,
    file_index: u32,
) -> bool {
    let cdh = match mz_zip_get_cdh(p_zip, file_index) {
        Some(data) if data.len() >= MZ_ZIP_CENTRAL_DIR_HEADER_SIZE => data,
        _ => return false,
    };
    let bit_flag = read_le16(cdh, MZ_ZIP_CDH_BIT_FLAG_OFS);
    let method = read_le16(cdh, MZ_ZIP_CDH_METHOD_OFS);
    let encrypted = (bit_flag & (MZ_ZIP_GENERAL_PURPOSE_BIT_FLAG_IS_ENCRYPTED as u16)) != 0;
    let patch = (bit_flag & (MZ_ZIP_GENERAL_PURPOSE_BIT_FLAG_COMPRESSED_PATCH_FLAG as u16)) != 0;
    !encrypted && !patch && (method == 0 || method == MZ_DEFLATED as u16)
}

// --- Reader locate (public wrappers) ---

/// Locate a file in the archive by name. Returns -1 if not found.
pub fn mz_zip_reader_locate_file(
    p_zip: &MzZipArchive,
    name: &str,
    comment: Option<&str>,
    flags: u32,
) -> i32 {
    match mz_zip_reader_locate_file_v2_pub(p_zip, name, comment, flags) {
        Some(idx) => idx as i32,
        None => -1,
    }
}

/// Locate a file in the archive by name (v2). Returns file index or None.
pub fn mz_zip_reader_locate_file_v2_pub(
    p_zip: &MzZipArchive,
    name: &str,
    comment: Option<&str>,
    flags: u32,
) -> Option<u32> {
    mz_zip_reader_locate_file_v2(p_zip, name, comment, flags)
}

// --- String comparison helpers ---

fn mz_zip_string_equal(a: &[u8], b: &[u8], flags: u32) -> bool {
    if a.len() != b.len() {
        return false;
    }
    if (flags & MZ_ZIP_FLAG_CASE_SENSITIVE) != 0 {
        a == b
    } else {
        a.iter().zip(b.iter()).all(|(&x, &y)| mz_tolower(x) == mz_tolower(y))
    }
}

// --- Reader extract: to memory ---

/// Extract a file to a caller-provided buffer (no internal alloc for read buf).
pub fn mz_zip_reader_extract_to_mem_no_alloc(
    p_zip: &MzZipArchive,
    file_index: u32,
    buf: &mut [u8],
    flags: u32,
) -> Result<(), MzZipError> {
    mz_zip_reader_extract_to_mem_internal(p_zip, file_index, buf, flags)
}

/// Extract a file (by name) to a caller-provided buffer.
pub fn mz_zip_reader_extract_file_to_mem_no_alloc(
    p_zip: &MzZipArchive,
    filename: &str,
    buf: &mut [u8],
    flags: u32,
) -> Result<(), MzZipError> {
    let idx = mz_zip_reader_locate_file_v2(p_zip, filename, None, flags)
        .ok_or(MzZipError::FileNotFound)?;
    mz_zip_reader_extract_to_mem_internal(p_zip, idx, buf, flags)
}

/// Extract a file to a caller-provided buffer.
pub fn mz_zip_reader_extract_to_mem(
    p_zip: &MzZipArchive,
    file_index: u32,
    buf: &mut [u8],
    flags: u32,
) -> Result<(), MzZipError> {
    mz_zip_reader_extract_to_mem_internal(p_zip, file_index, buf, flags)
}

/// Extract a file (by name) to a caller-provided buffer.
pub fn mz_zip_reader_extract_file_to_mem(
    p_zip: &MzZipArchive,
    filename: &str,
    buf: &mut [u8],
    flags: u32,
) -> Result<(), MzZipError> {
    let idx = mz_zip_reader_locate_file_v2(p_zip, filename, None, flags)
        .ok_or(MzZipError::FileNotFound)?;
    mz_zip_reader_extract_to_mem_internal(p_zip, idx, buf, flags)
}

/// Internal helper: extract file data into a provided buffer from memory-backed archive.
fn mz_zip_reader_extract_to_mem_internal(
    p_zip: &MzZipArchive,
    file_index: u32,
    buf: &mut [u8],
    flags: u32,
) -> Result<(), MzZipError> {
    let cdh = mz_zip_get_cdh(p_zip, file_index).ok_or(MzZipError::InvalidParameter)?;
    let mut file_stat = MzZipArchiveFileStat {
        m_file_index: 0, m_central_dir_ofs: 0, m_version_made_by: 0,
        m_version_needed: 0, m_bit_flag: 0, m_method: 0, m_crc32: 0,
        m_comp_size: 0, m_uncomp_size: 0, m_internal_attr: 0,
        m_external_attr: 0, m_local_header_ofs: 0, m_comment_size: 0,
        m_is_directory: false, m_is_encrypted: false, m_is_supported: false,
        m_filename: String::new(), m_comment: String::new(), m_time: 0,
    };
    if !mz_zip_file_stat_internal(p_zip, file_index, Some(cdh), &mut file_stat) {
        return Err(MzZipError::FileNotFound);
    }
    if file_stat.m_is_directory || file_stat.m_uncomp_size == 0 {
        return Ok(());
    }
    if file_stat.m_is_encrypted {
        return Err(MzZipError::UnsupportedEncryption);
    }
    if file_stat.m_method != 0 && file_stat.m_method != MZ_DEFLATED as u16 {
        return Err(MzZipError::UnsupportedMethod);
    }
    let needed = file_stat.m_uncomp_size as usize;
    if buf.len() < needed {
        return Err(MzZipError::BufTooSmall);
    }

    let state = p_zip.m_pstate.as_ref().ok_or(MzZipError::InternalError)?;
    let pmem = state.pmem.as_ref().ok_or(MzZipError::UnsupportedFeature)?;
    let start = (state.file_archive_start_ofs + file_stat.m_local_header_ofs) as usize;
    if start + MZ_ZIP_LOCAL_DIR_HEADER_SIZE > pmem.len() {
        return Err(MzZipError::FileReadFailed);
    }
    let local_header = &pmem[start..start + MZ_ZIP_LOCAL_DIR_HEADER_SIZE];
    let sig = read_le32(local_header, 0);
    if sig != MZ_ZIP_LOCAL_DIR_HEADER_SIG as u32 {
        return Err(MzZipError::InvalidHeaderOrCorrupted);
    }
    let fname_len = read_le16(local_header, MZ_ZIP_LDH_FILENAME_LEN_OFS) as u64;
    let extra_len = read_le16(local_header, MZ_ZIP_LDH_EXTRA_LEN_OFS) as u64;
    let data_ofs = start as u64 + MZ_ZIP_LOCAL_DIR_HEADER_SIZE as u64 + fname_len + extra_len;

    if file_stat.m_method == 0 {
        let ds = data_ofs as usize;
        let de = ds + needed;
        if de > pmem.len() {
            return Err(MzZipError::FileReadFailed);
        }
        buf[..needed].copy_from_slice(&pmem[ds..de]);
        let crc = mz_crc32(MZ_CRC32_INIT, &buf[..needed]);
        if crc != file_stat.m_crc32 {
            return Err(MzZipError::CrcCheckFailed);
        }
        Ok(())
    } else {
        // Deflated: decompress via tinfl
        let comp_start = data_ofs as usize;
        let comp_end = comp_start + file_stat.m_comp_size as usize;
        if comp_end > pmem.len() {
            return Err(MzZipError::FileReadFailed);
        }
        let compressed = &pmem[comp_start..comp_end];
        let decompressed = tinfl_decompress(compressed, needed)?;
        if decompressed.len() != needed {
            return Err(MzZipError::UnexpectedDecompressedSize);
        }
        buf[..needed].copy_from_slice(&decompressed);
        let crc = mz_crc32(MZ_CRC32_INIT, &buf[..needed]);
        if crc != file_stat.m_crc32 {
            return Err(MzZipError::CrcCheckFailed);
        }
        Ok(())
    }
}

/// Decompress raw DEFLATE data using the tinfl decompressor.
fn tinfl_decompress(compressed: &[u8], expected_size: usize) -> Result<Vec<u8>, MzZipError> {
    // Use flags=0 for raw deflate (no zlib header), decompress_to_vec handles
    // the USING_NON_WRAPPING_OUTPUT_BUF flag internally.
    let decompressed = crate::tinfl::decompress_to_vec(compressed, 0)
        .ok_or(MzZipError::DecompressionFailed)?;
    if decompressed.len() != expected_size {
        return Err(MzZipError::UnexpectedDecompressedSize);
    }
    Ok(decompressed)
}

/// Extract a file to a heap-allocated buffer (by name).
pub fn mz_zip_reader_extract_file_to_heap(
    p_zip: &MzZipArchive,
    filename: &str,
    flags: u32,
) -> Result<Vec<u8>, MzZipError> {
    let idx = mz_zip_reader_locate_file_v2(p_zip, filename, None, flags)
        .ok_or(MzZipError::FileNotFound)?;
    mz_zip_reader_extract_to_heap(p_zip, idx, flags)
}

// --- Reader extract: to callback ---

/// Write callback type: receives (offset, data) and returns bytes written.
pub type MzFileWriteFunc = fn(opaque: &mut dyn std::any::Any, file_ofs: u64, buf: &[u8]) -> usize;

/// Extract a file and send data through a callback.
pub fn mz_zip_reader_extract_to_callback(
    p_zip: &MzZipArchive,
    file_index: u32,
    callback: fn(&mut Vec<u8>, u64, &[u8]) -> usize,
    opaque: &mut Vec<u8>,
    flags: u32,
) -> Result<(), MzZipError> {
    let cdh = mz_zip_get_cdh(p_zip, file_index).ok_or(MzZipError::InvalidParameter)?;
    let mut file_stat = MzZipArchiveFileStat {
        m_file_index: 0, m_central_dir_ofs: 0, m_version_made_by: 0,
        m_version_needed: 0, m_bit_flag: 0, m_method: 0, m_crc32: 0,
        m_comp_size: 0, m_uncomp_size: 0, m_internal_attr: 0,
        m_external_attr: 0, m_local_header_ofs: 0, m_comment_size: 0,
        m_is_directory: false, m_is_encrypted: false, m_is_supported: false,
        m_filename: String::new(), m_comment: String::new(), m_time: 0,
    };
    if !mz_zip_file_stat_internal(p_zip, file_index, Some(cdh), &mut file_stat) {
        return Err(MzZipError::FileNotFound);
    }
    if file_stat.m_is_directory || file_stat.m_uncomp_size == 0 {
        return Ok(());
    }
    if file_stat.m_is_encrypted {
        return Err(MzZipError::UnsupportedEncryption);
    }
    if file_stat.m_method != 0 && file_stat.m_method != MZ_DEFLATED as u16 {
        return Err(MzZipError::UnsupportedMethod);
    }

    let state = p_zip.m_pstate.as_ref().ok_or(MzZipError::InternalError)?;
    let pmem = state.pmem.as_ref().ok_or(MzZipError::UnsupportedFeature)?;
    let start = (state.file_archive_start_ofs + file_stat.m_local_header_ofs) as usize;
    if start + MZ_ZIP_LOCAL_DIR_HEADER_SIZE > pmem.len() {
        return Err(MzZipError::FileReadFailed);
    }
    let lh = &pmem[start..start + MZ_ZIP_LOCAL_DIR_HEADER_SIZE];
    if read_le32(lh, 0) != MZ_ZIP_LOCAL_DIR_HEADER_SIG as u32 {
        return Err(MzZipError::InvalidHeaderOrCorrupted);
    }
    let fname_len = read_le16(lh, MZ_ZIP_LDH_FILENAME_LEN_OFS) as u64;
    let extra_len = read_le16(lh, MZ_ZIP_LDH_EXTRA_LEN_OFS) as u64;
    let data_ofs = start as u64 + MZ_ZIP_LOCAL_DIR_HEADER_SIZE as u64 + fname_len + extra_len;

    if file_stat.m_method == 0 {
        let ds = data_ofs as usize;
        let de = ds + file_stat.m_uncomp_size as usize;
        if de > pmem.len() {
            return Err(MzZipError::FileReadFailed);
        }
        let data = &pmem[ds..de];
        let crc = mz_crc32(MZ_CRC32_INIT, data);
        if crc != file_stat.m_crc32 {
            return Err(MzZipError::CrcCheckFailed);
        }
        let written = callback(opaque, 0, data);
        if written != data.len() {
            return Err(MzZipError::WriteCallbackFailed);
        }
        Ok(())
    } else {
        // Deflated: decompress then pass to callback
        let cs = data_ofs as usize;
        let ce = cs + file_stat.m_comp_size as usize;
        if ce > pmem.len() {
            return Err(MzZipError::FileReadFailed);
        }
        let compressed = &pmem[cs..ce];
        let needed = file_stat.m_uncomp_size as usize;
        let decompressed = tinfl_decompress(compressed, needed)?;
        let crc = mz_crc32(MZ_CRC32_INIT, &decompressed);
        if crc != file_stat.m_crc32 {
            return Err(MzZipError::CrcCheckFailed);
        }
        let written = callback(opaque, 0, &decompressed);
        if written != decompressed.len() {
            return Err(MzZipError::WriteCallbackFailed);
        }
        Ok(())
    }
}

/// Extract a file (by name) and send data through a callback.
pub fn mz_zip_reader_extract_file_to_callback(
    p_zip: &MzZipArchive,
    filename: &str,
    callback: fn(&mut Vec<u8>, u64, &[u8]) -> usize,
    opaque: &mut Vec<u8>,
    flags: u32,
) -> Result<(), MzZipError> {
    let idx = mz_zip_reader_locate_file_v2(p_zip, filename, None, flags)
        .ok_or(MzZipError::FileNotFound)?;
    mz_zip_reader_extract_to_callback(p_zip, idx, callback, opaque, flags)
}

// --- Reader extract: iterator ---

/// Create a new extraction iterator for a file by index.
pub fn mz_zip_reader_extract_iter_new(
    p_zip: &MzZipArchive,
    file_index: u32,
    flags: u32,
) -> Result<MzZipReaderExtractIterState, MzZipError> {
    let cdh = mz_zip_get_cdh(p_zip, file_index).ok_or(MzZipError::InvalidParameter)?;
    let mut file_stat = MzZipArchiveFileStat {
        m_file_index: 0, m_central_dir_ofs: 0, m_version_made_by: 0,
        m_version_needed: 0, m_bit_flag: 0, m_method: 0, m_crc32: 0,
        m_comp_size: 0, m_uncomp_size: 0, m_internal_attr: 0,
        m_external_attr: 0, m_local_header_ofs: 0, m_comment_size: 0,
        m_is_directory: false, m_is_encrypted: false, m_is_supported: false,
        m_filename: String::new(), m_comment: String::new(), m_time: 0,
    };
    if !mz_zip_file_stat_internal(p_zip, file_index, Some(cdh), &mut file_stat) {
        return Err(MzZipError::FileNotFound);
    }
    if file_stat.m_is_encrypted {
        return Err(MzZipError::UnsupportedEncryption);
    }
    if file_stat.m_method != 0 && file_stat.m_method != MZ_DEFLATED as u16 {
        return Err(MzZipError::UnsupportedMethod);
    }

    // Pre-extract the entire file into pread_buf for simple iteration
    let state = p_zip.m_pstate.as_ref().ok_or(MzZipError::InternalError)?;
    let pmem = state.pmem.as_ref().ok_or(MzZipError::UnsupportedFeature)?;
    let start = (state.file_archive_start_ofs + file_stat.m_local_header_ofs) as usize;
    if start + MZ_ZIP_LOCAL_DIR_HEADER_SIZE > pmem.len() {
        return Err(MzZipError::FileReadFailed);
    }
    let lh = &pmem[start..start + MZ_ZIP_LOCAL_DIR_HEADER_SIZE];
    if read_le32(lh, 0) != MZ_ZIP_LOCAL_DIR_HEADER_SIG as u32 {
        return Err(MzZipError::InvalidHeaderOrCorrupted);
    }
    let fname_len = read_le16(lh, MZ_ZIP_LDH_FILENAME_LEN_OFS) as u64;
    let extra_len = read_le16(lh, MZ_ZIP_LDH_EXTRA_LEN_OFS) as u64;
    let data_ofs = start as u64 + MZ_ZIP_LOCAL_DIR_HEADER_SIZE as u64 + fname_len + extra_len;

    let extracted_data = if file_stat.m_is_directory || file_stat.m_uncomp_size == 0 {
        Vec::new()
    } else if file_stat.m_method == 0 {
        let ds = data_ofs as usize;
        let de = ds + file_stat.m_uncomp_size as usize;
        if de > pmem.len() {
            return Err(MzZipError::FileReadFailed);
        }
        pmem[ds..de].to_vec()
    } else {
        // Deflated: decompress
        let cs = data_ofs as usize;
        let ce = cs + file_stat.m_comp_size as usize;
        if ce > pmem.len() {
            return Err(MzZipError::FileReadFailed);
        }
        let compressed = &pmem[cs..ce];
        let needed = file_stat.m_uncomp_size as usize;
        tinfl_decompress(compressed, needed)?
    };

    Ok(MzZipReaderExtractIterState {
        pzip: None,
        file_stat,
        file_crc32: 0,
        status: 0,
        pread_buf: Some(extracted_data),
        pwrite_buf: None,
    })
}

/// Create a new extraction iterator for a file by name.
pub fn mz_zip_reader_extract_file_iter_new(
    p_zip: &MzZipArchive,
    filename: &str,
    flags: u32,
) -> Result<MzZipReaderExtractIterState, MzZipError> {
    let idx = mz_zip_reader_locate_file_v2(p_zip, filename, None, flags)
        .ok_or(MzZipError::FileNotFound)?;
    mz_zip_reader_extract_iter_new(p_zip, idx, flags)
}

/// Read bytes from an extraction iterator.
pub fn mz_zip_reader_extract_iter_read(
    iter_state: &mut MzZipReaderExtractIterState,
    buf: &mut [u8],
) -> usize {
    let data = match iter_state.pread_buf.as_ref() {
        Some(d) => d,
        None => return 0,
    };
    let offset = iter_state.status as usize;
    if offset >= data.len() {
        return 0;
    }
    let available = data.len() - offset;
    let to_copy = buf.len().min(available);
    buf[..to_copy].copy_from_slice(&data[offset..offset + to_copy]);
    iter_state.status += to_copy as i32;
    iter_state.file_crc32 = mz_crc32(iter_state.file_crc32 as u64, &buf[..to_copy]);
    to_copy
}

/// Free an extraction iterator and check CRC.
pub fn mz_zip_reader_extract_iter_free(
    iter_state: MzZipReaderExtractIterState,
) -> bool {
    if iter_state.file_stat.m_uncomp_size == 0 {
        return true;
    }
    if let Some(ref data) = iter_state.pread_buf {
        let expected_crc = mz_crc32(MZ_CRC32_INIT, data);
        expected_crc == iter_state.file_stat.m_crc32
    } else {
        true
    }
}

// --- Reader extract: to file ---

/// Extract a file to a disk file.
pub fn mz_zip_reader_extract_to_file(
    p_zip: &MzZipArchive,
    file_index: u32,
    dst_filename: &str,
    flags: u32,
) -> Result<(), MzZipError> {
    let data = mz_zip_reader_extract_to_heap(p_zip, file_index, flags)?;
    let mut file = File::create(dst_filename).map_err(|_| MzZipError::FileCreateFailed)?;
    file.write_all(&data).map_err(|_| MzZipError::FileWriteFailed)?;
    Ok(())
}

/// Extract a file (by name) to a disk file.
pub fn mz_zip_reader_extract_file_to_file(
    p_zip: &MzZipArchive,
    archive_filename: &str,
    dst_filename: &str,
    flags: u32,
) -> Result<(), MzZipError> {
    let idx = mz_zip_reader_locate_file_v2(p_zip, archive_filename, None, flags)
        .ok_or(MzZipError::FileNotFound)?;
    mz_zip_reader_extract_to_file(p_zip, idx, dst_filename, flags)
}

/// Extract a file to a cfile (same as extract_to_file, using Write trait).
pub fn mz_zip_reader_extract_to_cfile(
    p_zip: &MzZipArchive,
    file_index: u32,
    writer: &mut dyn Write,
    flags: u32,
) -> Result<(), MzZipError> {
    let data = mz_zip_reader_extract_to_heap(p_zip, file_index, flags)?;
    writer.write_all(&data).map_err(|_| MzZipError::FileWriteFailed)?;
    Ok(())
}

/// Extract a file (by name) to a cfile.
pub fn mz_zip_reader_extract_file_to_cfile(
    p_zip: &MzZipArchive,
    archive_filename: &str,
    writer: &mut dyn Write,
    flags: u32,
) -> Result<(), MzZipError> {
    let idx = mz_zip_reader_locate_file_v2(p_zip, archive_filename, None, flags)
        .ok_or(MzZipError::FileNotFound)?;
    mz_zip_reader_extract_to_cfile(p_zip, idx, writer, flags)
}

// --- Reader init variants ---

/// Initialize a reader from an archive with user-supplied read callback.
pub fn mz_zip_reader_init(
    p_zip: &mut MzZipArchive,
    size: u64,
    flags: u32,
) -> Result<(), MzZipError> {
    if p_zip.m_pread.is_none() {
        return Err(MzZipError::InvalidParameter);
    }
    p_zip.m_zip_mode = MzZipMode::Invalid;
    p_zip.init_reader()?;
    if let Some(state) = &mut p_zip.m_pstate {
        state.init_flags = flags;
    }
    p_zip.m_zip_type = MzZipType::User;
    p_zip.m_archive_size = size;
    if !mz_zip_reader_read_central_dir(p_zip, flags) {
        let err = p_zip.m_last_error;
        mz_zip_reader_end_internal(p_zip, false);
        return Err(err);
    }
    Ok(())
}

/// Initialize a reader from an in-memory buffer.
pub fn mz_zip_reader_init_mem(
    p_zip: &mut MzZipArchive,
    data: &[u8],
    flags: u32,
) -> Result<(), MzZipError> {
    if data.is_empty() {
        return Err(MzZipError::InvalidParameter);
    }
    p_zip.m_zip_mode = MzZipMode::Invalid;
    p_zip.init_reader()?;
    if let Some(state) = &mut p_zip.m_pstate {
        state.pmem = Some(data.to_vec());
        state.mem_size = data.len();
        state.mem_capacity = data.len();
        state.init_flags = flags;
        state.file_archive_start_ofs = 0;
    }
    p_zip.m_zip_type = MzZipType::Memory;
    p_zip.m_archive_size = data.len() as u64;
    if !mz_zip_reader_read_central_dir(p_zip, flags) {
        let err = p_zip.m_last_error;
        mz_zip_reader_end_internal(p_zip, false);
        return Err(err);
    }
    Ok(())
}

/// Initialize a reader from a file on disk.
pub fn mz_zip_reader_init_file(
    p_zip: &mut MzZipArchive,
    filename: &str,
    flags: u32,
) -> Result<(), MzZipError> {
    mz_zip_reader_init_file_v2_pub(p_zip, filename, flags, 0, 0)
}

/// Initialize a reader from a file on disk (v2 with offset/size).
pub fn mz_zip_reader_init_file_v2_pub(
    p_zip: &mut MzZipArchive,
    filename: &str,
    flags: u32,
    file_start_ofs: u64,
    archive_size: u64,
) -> Result<(), MzZipError> {
    let file = File::open(filename).map_err(|_| MzZipError::FileOpenFailed)?;
    mz_zip_reader_init_file_v2(p_zip, &file, flags, file_start_ofs, archive_size)
}

/// Initialize a reader from an open file handle.
pub fn mz_zip_reader_init_cfile(
    p_zip: &mut MzZipArchive,
    file: &File,
    archive_size: u64,
    flags: u32,
) -> Result<(), MzZipError> {
    mz_zip_reader_init_file_v2(p_zip, file, flags, 0, archive_size)
}

// --- Misc: zero struct ---

/// Clear a zip archive to all zeros.
pub fn mz_zip_zero_struct(p_zip: &mut MzZipArchive) {
    *p_zip = MzZipArchive::new();
}

// --- Validation functions ---

/// Validate a single file in the archive by comparing local header to central directory.
pub fn mz_zip_validate_file(
    p_zip: &MzZipArchive,
    file_index: u32,
    flags: u32,
) -> Result<bool, MzZipError> {
    let cdh = mz_zip_get_cdh(p_zip, file_index).ok_or(MzZipError::InvalidParameter)?;
    let mut file_stat = MzZipArchiveFileStat {
        m_file_index: 0, m_central_dir_ofs: 0, m_version_made_by: 0,
        m_version_needed: 0, m_bit_flag: 0, m_method: 0, m_crc32: 0,
        m_comp_size: 0, m_uncomp_size: 0, m_internal_attr: 0,
        m_external_attr: 0, m_local_header_ofs: 0, m_comment_size: 0,
        m_is_directory: false, m_is_encrypted: false, m_is_supported: false,
        m_filename: String::new(), m_comment: String::new(), m_time: 0,
    };
    if !mz_zip_file_stat_internal(p_zip, file_index, Some(cdh), &mut file_stat) {
        return Err(MzZipError::InvalidParameter);
    }
    if file_stat.m_is_directory || file_stat.m_uncomp_size == 0 {
        return Ok(true);
    }
    if file_stat.m_is_encrypted {
        return Err(MzZipError::UnsupportedEncryption);
    }
    if !file_stat.m_is_supported {
        return Err(MzZipError::UnsupportedFeature);
    }

    // Read the local header
    let state = p_zip.m_pstate.as_ref().ok_or(MzZipError::InternalError)?;
    let pmem = state.pmem.as_ref().ok_or(MzZipError::UnsupportedFeature)?;
    let lh_start = (state.file_archive_start_ofs + file_stat.m_local_header_ofs) as usize;
    if lh_start + MZ_ZIP_LOCAL_DIR_HEADER_SIZE > pmem.len() {
        return Err(MzZipError::FileReadFailed);
    }
    let lh = &pmem[lh_start..lh_start + MZ_ZIP_LOCAL_DIR_HEADER_SIZE];
    if read_le32(lh, 0) != MZ_ZIP_LOCAL_DIR_HEADER_SIG as u32 {
        return Err(MzZipError::InvalidHeaderOrCorrupted);
    }

    let lh_fname_len = read_le16(lh, MZ_ZIP_LDH_FILENAME_LEN_OFS) as usize;
    let lh_extra_len = read_le16(lh, MZ_ZIP_LDH_EXTRA_LEN_OFS) as usize;

    // Verify filename length matches central directory
    if lh_fname_len != file_stat.m_filename.len() {
        return Err(MzZipError::ValidationFailed);
    }

    // Verify filename bytes match
    if lh_fname_len > 0 {
        let fname_start = lh_start + MZ_ZIP_LOCAL_DIR_HEADER_SIZE;
        if fname_start + lh_fname_len > pmem.len() {
            return Err(MzZipError::FileReadFailed);
        }
        let lh_fname = &pmem[fname_start..fname_start + lh_fname_len];
        if lh_fname != file_stat.m_filename.as_bytes() {
            return Err(MzZipError::ValidationFailed);
        }
    }

    // Check local header sizes match CDH (unless data descriptor is used)
    let lh_bit_flags = read_le16(lh, MZ_ZIP_LDH_BIT_FLAG_OFS);
    let has_data_descriptor = (lh_bit_flags & 8) != 0;

    if !has_data_descriptor {
        let lh_crc32 = read_le32(lh, MZ_ZIP_LDH_CRC32_OFS);
        let mut lh_comp_size = read_le32(lh, MZ_ZIP_LDH_COMPRESSED_SIZE_OFS) as u64;
        let mut lh_uncomp_size = read_le32(lh, MZ_ZIP_LDH_DECOMPRESSED_SIZE_OFS) as u64;

        // Handle zip64 extended information in local header extra data
        if lh_extra_len > 0 && (lh_comp_size == 0xFFFFFFFF || lh_uncomp_size == 0xFFFFFFFF) {
            let extra_start = lh_start + MZ_ZIP_LOCAL_DIR_HEADER_SIZE + lh_fname_len;
            if extra_start + lh_extra_len <= pmem.len() {
                let extra_data = &pmem[extra_start..extra_start + lh_extra_len];
                let mut pos = 0;
                while pos + 4 <= extra_data.len() {
                    let field_id = read_le16(extra_data, pos);
                    let field_size = read_le16(extra_data, pos + 2) as usize;
                    if pos + 4 + field_size > extra_data.len() { break; }
                    if field_id == MZ_ZIP64_EXTENDED_INFORMATION_FIELD_HEADER_ID as u16 && field_size >= 16 {
                        lh_uncomp_size = read_le64(extra_data, pos + 4);
                        lh_comp_size = read_le64(extra_data, pos + 12);
                        break;
                    }
                    pos += 4 + field_size;
                }
            }
        }

        if lh_crc32 != file_stat.m_crc32
            || lh_comp_size != file_stat.m_comp_size
            || lh_uncomp_size != file_stat.m_uncomp_size
        {
            return Err(MzZipError::ValidationFailed);
        }
    }

    // Optionally validate data by extracting and checking CRC
    if (flags & MZ_ZIP_FLAG_VALIDATE_HEADERS_ONLY) == 0 {
        let needed = file_stat.m_uncomp_size as usize;
        let mut buf = vec![0u8; needed];
        mz_zip_reader_extract_to_mem_internal(p_zip, file_index, &mut buf, 0)?;
        // CRC is checked inside extract_to_mem_internal
    }

    Ok(true)
}

/// Validate an entire archive by validating each file.
pub fn mz_zip_validate_archive(
    p_zip: &MzZipArchive,
    flags: u32,
) -> Result<bool, MzZipError> {
    let total = p_zip.m_total_files;
    for i in 0..total {
        if (flags & MZ_ZIP_FLAG_VALIDATE_LOCATE_FILE_FLAG) != 0 {
            let cdh = mz_zip_get_cdh(p_zip, i).ok_or(MzZipError::InvalidParameter)?;
            let mut stat = MzZipArchiveFileStat {
                m_file_index: 0, m_central_dir_ofs: 0, m_version_made_by: 0,
                m_version_needed: 0, m_bit_flag: 0, m_method: 0, m_crc32: 0,
                m_comp_size: 0, m_uncomp_size: 0, m_internal_attr: 0,
                m_external_attr: 0, m_local_header_ofs: 0, m_comment_size: 0,
                m_is_directory: false, m_is_encrypted: false, m_is_supported: false,
                m_filename: String::new(), m_comment: String::new(), m_time: 0,
            };
            if mz_zip_file_stat_internal(p_zip, i, Some(cdh), &mut stat) {
                let found = mz_zip_reader_locate_file_v2(p_zip, &stat.m_filename, None, 0);
                if found != Some(i) {
                    return Err(MzZipError::ValidationFailed);
                }
            }
        }
        mz_zip_validate_file(p_zip, i, flags)?;
    }
    Ok(true)
}

/// Validate an in-memory archive.
pub fn mz_zip_validate_mem_archive(
    data: &[u8],
    flags: u32,
) -> Result<bool, MzZipError> {
    let mut zip = MzZipArchive::new();
    mz_zip_reader_init_mem(&mut zip, data, flags)?;
    let result = mz_zip_validate_archive(&zip, flags);
    mz_zip_reader_end_internal(&mut zip, result.is_ok());
    result
}

/// Validate a file-based archive.
pub fn mz_zip_validate_file_archive(
    filename: &str,
    flags: u32,
) -> Result<bool, MzZipError> {
    let mut zip = MzZipArchive::new();
    mz_zip_reader_init_file(&mut zip, filename, flags)?;
    let result = mz_zip_validate_archive(&zip, flags);
    mz_zip_reader_end_internal(&mut zip, result.is_ok());
    result
}

// --- Writer init functions ---

impl MzZipArchive {
    /// Initialize writer (v2 with flags).
    pub fn mz_zip_writer_init_v2(
        &mut self,
        existing_size: u64,
        flags: u32,
    ) -> Result<(), MzZipError> {
        if self.m_pstate.is_some() || self.m_zip_mode != MzZipMode::Invalid {
            return Err(MzZipError::InvalidParameter);
        }
        if self.m_pwrite.is_none() {
            return Err(MzZipError::InvalidParameter);
        }
        if (flags & MZ_ZIP_FLAG_WRITE_ALLOW_READING) != 0 && self.m_pread.is_none() {
            return Err(MzZipError::InvalidParameter);
        }
        if self.m_file_offset_alignment != 0
            && (self.m_file_offset_alignment & (self.m_file_offset_alignment - 1)) != 0
        {
            return Err(MzZipError::InvalidParameter);
        }

        self.m_archive_size = existing_size;
        self.m_central_directory_file_ofs = 0;
        self.m_total_files = 0;

        let zip64 = (flags & MZ_ZIP_FLAG_WRITE_ZIP64) != 0;
        let mut state = Box::new(MzZipInternalState {
            central_dir: MzZipArray::new(1),
            central_dir_offsets: MzZipArray::new(4),
            sorted_central_dir_offsets: MzZipArray::new(4),
            init_flags: flags,
            zip64,
            zip64_has_extended_info_fields: zip64,
            pfile: None,
            file_archive_start_ofs: 0,
            pmem: None,
            mem_size: 0,
            mem_capacity: 0,
        });
        self.m_pstate = Some(state);
        self.m_zip_type = MzZipType::User;
        self.m_zip_mode = MzZipMode::Writing;
        Ok(())
    }

    /// Initialize writer (simple, no flags).
    pub fn mz_zip_writer_init(
        &mut self,
        existing_size: u64,
    ) -> Result<(), MzZipError> {
        self.mz_zip_writer_init_v2(existing_size, 0)
    }

    /// Initialize a heap-based writer (v2 with flags).
    pub fn mz_zip_writer_init_heap_v2(
        &mut self,
        size_to_reserve: usize,
        initial_alloc: usize,
        flags: u32,
    ) -> Result<(), MzZipError> {
        // Set up heap write function
        self.m_pwrite = Some(|_zip, _ofs, buf| buf.len());
        if (flags & MZ_ZIP_FLAG_WRITE_ALLOW_READING) != 0 {
            self.m_pread = Some(|_zip, _ofs, buf| buf.len());
        }
        self.mz_zip_writer_init_v2(size_to_reserve as u64, flags)?;
        self.m_zip_type = MzZipType::Heap;

        let alloc_size = initial_alloc.max(size_to_reserve);
        if alloc_size > 0 {
            if let Some(ref mut state) = self.m_pstate {
                state.pmem = Some(vec![0u8; alloc_size]);
                state.mem_size = alloc_size;
                state.mem_capacity = alloc_size;
            }
        }
        Ok(())
    }

    /// Initialize a heap-based writer (simple).
    pub fn mz_zip_writer_init_heap(
        &mut self,
        size_to_reserve: usize,
        initial_alloc: usize,
    ) -> Result<(), MzZipError> {
        self.mz_zip_writer_init_heap_v2(size_to_reserve, initial_alloc, 0)
    }

    /// Initialize a file-based writer (v2 with flags).
    pub fn mz_zip_writer_init_file_v2(
        &mut self,
        filename: &str,
        size_to_reserve: u64,
        flags: u32,
    ) -> Result<(), MzZipError> {
        self.m_pwrite = Some(|_zip, _ofs, buf| buf.len());
        if (flags & MZ_ZIP_FLAG_WRITE_ALLOW_READING) != 0 {
            self.m_pread = Some(|_zip, _ofs, buf| buf.len());
        }
        self.mz_zip_writer_init_v2(size_to_reserve, flags)?;

        let file = if (flags & MZ_ZIP_FLAG_WRITE_ALLOW_READING) != 0 {
            std::fs::OpenOptions::new()
                .read(true).write(true).create(true).truncate(true)
                .open(filename)
        } else {
            std::fs::OpenOptions::new()
                .write(true).create(true).truncate(true)
                .open(filename)
        };
        let file = file.map_err(|_| MzZipError::FileOpenFailed)?;

        if let Some(ref mut state) = self.m_pstate {
            state.pfile = Some(file);
        }
        self.m_zip_type = MzZipType::File;

        if size_to_reserve > 0 {
            let zeros = vec![0u8; 4096];
            let mut remaining = size_to_reserve;
            let mut ofs = 0u64;
            while remaining > 0 {
                let n = (zeros.len() as u64).min(remaining) as usize;
                let written = self.write_data(ofs, &zeros[..n])
                    .map_err(|_| MzZipError::FileWriteFailed)?;
                if written != n {
                    return Err(MzZipError::FileWriteFailed);
                }
                ofs += n as u64;
                remaining -= n as u64;
            }
        }
        Ok(())
    }

    /// Initialize a file-based writer (simple).
    pub fn mz_zip_writer_init_file(
        &mut self,
        filename: &str,
        size_to_reserve: u64,
    ) -> Result<(), MzZipError> {
        self.mz_zip_writer_init_file_v2(filename, size_to_reserve, 0)
    }
}

// --- Writer: add_from_zip_reader ---

impl MzZipArchive {
    /// Add a file from another zip archive by cloning its compressed data.
    pub fn mz_zip_writer_add_from_zip_reader(
        &mut self,
        source_zip: &MzZipArchive,
        src_file_index: u32,
    ) -> Result<(), MzZipError> {
        if self.m_zip_mode != MzZipMode::Writing {
            return Err(MzZipError::InvalidParameter);
        }
        let src_cdh = mz_zip_get_cdh(source_zip, src_file_index)
            .ok_or(MzZipError::InvalidParameter)?;
        if src_cdh.len() < MZ_ZIP_CENTRAL_DIR_HEADER_SIZE {
            return Err(MzZipError::InvalidHeaderOrCorrupted);
        }
        if read_le32(src_cdh, MZ_ZIP_CDH_SIG_OFS) != MZ_ZIP_CENTRAL_DIR_HEADER_SIG as u32 {
            return Err(MzZipError::InvalidHeaderOrCorrupted);
        }

        let src_fname_len = read_le16(src_cdh, MZ_ZIP_CDH_FILENAME_LEN_OFS) as usize;
        let src_ext_len = read_le16(src_cdh, MZ_ZIP_CDH_EXTRA_LEN_OFS) as usize;
        let src_comment_len = read_le16(src_cdh, MZ_ZIP_CDH_COMMENT_LEN_OFS) as usize;
        let following_data_size = src_fname_len + src_ext_len + src_comment_len;

        // Copy entire CDH + following data
        let cdh_total = MZ_ZIP_CENTRAL_DIR_HEADER_SIZE + following_data_size;
        if cdh_total > src_cdh.len() {
            return Err(MzZipError::InvalidHeaderOrCorrupted);
        }
        let cdh_owned = src_cdh[..cdh_total].to_vec();

        // Get source file stat
        let mut src_stat = MzZipArchiveFileStat {
            m_file_index: 0, m_central_dir_ofs: 0, m_version_made_by: 0,
            m_version_needed: 0, m_bit_flag: 0, m_method: 0, m_crc32: 0,
            m_comp_size: 0, m_uncomp_size: 0, m_internal_attr: 0,
            m_external_attr: 0, m_local_header_ofs: 0, m_comment_size: 0,
            m_is_directory: false, m_is_encrypted: false, m_is_supported: false,
            m_filename: String::new(), m_comment: String::new(), m_time: 0,
        };
        if !mz_zip_file_stat_internal(source_zip, src_file_index, Some(src_cdh), &mut src_stat) {
            return Err(MzZipError::FileNotFound);
        }

        // Read source local header + data from source memory
        let src_state = source_zip.m_pstate.as_ref().ok_or(MzZipError::InternalError)?;
        let src_mem = src_state.pmem.as_ref().ok_or(MzZipError::UnsupportedFeature)?;
        let src_lh_start = (src_state.file_archive_start_ofs + src_stat.m_local_header_ofs) as usize;
        if src_lh_start + MZ_ZIP_LOCAL_DIR_HEADER_SIZE > src_mem.len() {
            return Err(MzZipError::FileReadFailed);
        }
        let src_lh = &src_mem[src_lh_start..src_lh_start + MZ_ZIP_LOCAL_DIR_HEADER_SIZE];
        if read_le32(src_lh, 0) != MZ_ZIP_LOCAL_DIR_HEADER_SIG as u32 {
            return Err(MzZipError::InvalidHeaderOrCorrupted);
        }
        let lh_fname_len = read_le16(src_lh, MZ_ZIP_LDH_FILENAME_LEN_OFS) as usize;
        let lh_extra_len = read_le16(src_lh, MZ_ZIP_LDH_EXTRA_LEN_OFS) as usize;

        // Total bytes to copy: local header + fname + extra + compressed data
        let total_src_bytes = MZ_ZIP_LOCAL_DIR_HEADER_SIZE + lh_fname_len + lh_extra_len
            + src_stat.m_comp_size as usize;
        let src_end = src_lh_start + total_src_bytes;
        if src_end > src_mem.len() {
            return Err(MzZipError::FileReadFailed);
        }
        let src_bytes = &src_mem[src_lh_start..src_end];

        // Write alignment padding
        let padding = self.mz_zip_writer_compute_padding_needed_for_file_alignment();
        if padding > 0 {
            let ofs = self.m_archive_size;
            self.mz_zip_writer_write_zeros(ofs, padding)?;
            self.m_archive_size += padding as u64;
        }

        let local_dir_header_ofs = self.m_archive_size;

        // Write the copied data to dest
        let written = self.write_data(self.m_archive_size, src_bytes)?;
        if written != src_bytes.len() {
            return Err(MzZipError::FileWriteFailed);
        }
        self.m_archive_size += written as u64;

        // Handle data descriptor if present
        let bit_flags = read_le16(src_lh, MZ_ZIP_LDH_BIT_FLAG_OFS);
        if (bit_flags & 8) != 0 {
            let desc_start = src_end;
            // Try to read the data descriptor (at most 24 bytes for zip64)
            let desc_max = 24;
            if desc_start + desc_max <= src_mem.len() {
                let desc_data = &src_mem[desc_start..desc_start + desc_max];
                let has_id = read_le32(desc_data, 0) == MZ_ZIP_DATA_DESCRIPTOR_ID as u32;
                let is_zip64 = self.m_pstate.as_ref()
                    .map(|s| s.zip64).unwrap_or(false);
                let desc_size = if is_zip64 {
                    if has_id { 24 } else { 20 }
                } else if has_id { 16 } else { 12 };
                let written = self.write_data(self.m_archive_size, &desc_data[..desc_size])?;
                if written != desc_size {
                    return Err(MzZipError::FileWriteFailed);
                }
                self.m_archive_size += written as u64;
            }
        }

        // Add updated central directory entry
        let mut new_cdh = cdh_owned.clone();
        // Update local header offset in CDH
        if new_cdh.len() >= MZ_ZIP_CDH_LOCAL_HEADER_OFS + 4 {
            write_le32(&mut new_cdh, MZ_ZIP_CDH_LOCAL_HEADER_OFS, local_dir_header_ofs as u32);
        }

        // Append CDH to our central directory
        let state = self.m_pstate.as_mut().ok_or(MzZipError::InternalError)?;
        let cdir_ofs = state.central_dir.size as u32;
        // Grow central_dir by pushing all CDH bytes
        if let Some(ref mut vec) = state.central_dir.p {
            vec.extend_from_slice(&new_cdh);
            state.central_dir.size = vec.len();
            state.central_dir.capacity = vec.capacity();
        } else {
            state.central_dir.p = Some(new_cdh);
            state.central_dir.size = cdh_total;
            state.central_dir.capacity = cdh_total;
        }

        // Push offset to central_dir_offsets
        if let Some(ref mut vec) = state.central_dir_offsets.p {
            vec.extend_from_slice(&cdir_ofs.to_le_bytes());
            state.central_dir_offsets.size += 1;
            state.central_dir_offsets.capacity = state.central_dir_offsets.size;
        } else {
            state.central_dir_offsets.p = Some(cdir_ofs.to_le_bytes().to_vec());
            state.central_dir_offsets.size = 1;
            state.central_dir_offsets.capacity = 1;
        }

        self.m_total_files += 1;
        Ok(())
    }
}

// --- Writer: finalize ---

impl MzZipArchive {
    /// Finalize the archive by writing the central directory and EOCD.
    pub fn mz_zip_writer_finalize_archive(&mut self) -> Result<(), MzZipError> {
        if self.m_zip_mode != MzZipMode::Writing {
            return Err(MzZipError::InvalidParameter);
        }
        let is_zip64 = self.m_pstate.as_ref()
            .map(|s| s.zip64).unwrap_or(false);

        let mut central_dir_ofs = 0u64;
        let mut central_dir_size = 0u64;

        if self.m_total_files > 0 {
            central_dir_ofs = self.m_archive_size;
            self.m_central_directory_file_ofs = central_dir_ofs;

            let cdir_bytes = {
                let state = self.m_pstate.as_ref().ok_or(MzZipError::InternalError)?;
                let bytes = state.central_dir.as_bytes().ok_or(MzZipError::InternalError)?;
                central_dir_size = state.central_dir.size as u64;
                bytes[..central_dir_size as usize].to_vec()
            };

            let written = self.write_data(central_dir_ofs, &cdir_bytes)?;
            if written as u64 != central_dir_size {
                return Err(MzZipError::FileWriteFailed);
            }
            self.m_archive_size += central_dir_size;
        }

        if is_zip64 {
            // Write ZIP64 end of central directory header
            let mut hdr = [0u8; MZ_ZIP64_END_OF_CENTRAL_DIR_HEADER_SIZE];
            write_le32(&mut hdr, MZ_ZIP64_ECDH_SIG_OFS, MZ_ZIP64_END_OF_CENTRAL_DIR_HEADER_SIG as u32);
            write_le64(&mut hdr, MZ_ZIP64_ECDH_SIZE_OF_RECORD_OFS,
                (MZ_ZIP64_END_OF_CENTRAL_DIR_HEADER_SIZE - 12) as u64);
            write_le16(&mut hdr, MZ_ZIP64_ECDH_VERSION_MADE_BY_OFS, 0x031E);
            write_le16(&mut hdr, MZ_ZIP64_ECDH_VERSION_NEEDED_OFS, 0x002D);
            write_le64(&mut hdr, MZ_ZIP64_ECDH_CDIR_NUM_ENTRIES_ON_DISK_OFS, self.m_total_files as u64);
            write_le64(&mut hdr, MZ_ZIP64_ECDH_CDIR_TOTAL_ENTRIES_OFS, self.m_total_files as u64);
            write_le64(&mut hdr, MZ_ZIP64_ECDH_CDIR_SIZE_OFS, central_dir_size);
            write_le64(&mut hdr, MZ_ZIP64_ECDH_CDIR_OFS_OFS, central_dir_ofs);

            let rel_ofs = self.m_archive_size;
            let written = self.write_data(self.m_archive_size, &hdr)?;
            if written != MZ_ZIP64_END_OF_CENTRAL_DIR_HEADER_SIZE {
                return Err(MzZipError::FileWriteFailed);
            }
            self.m_archive_size += MZ_ZIP64_END_OF_CENTRAL_DIR_HEADER_SIZE as u64;

            // Write ZIP64 end of central directory locator
            let mut loc = [0u8; MZ_ZIP64_END_OF_CENTRAL_DIR_LOCATOR_SIZE];
            write_le32(&mut loc, MZ_ZIP64_ECDL_SIG_OFS, MZ_ZIP64_END_OF_CENTRAL_DIR_LOCATOR_SIG as u32);
            write_le64(&mut loc, MZ_ZIP64_ECDL_REL_OFS_TO_ZIP64_ECDR_OFS, rel_ofs);
            write_le32(&mut loc, MZ_ZIP64_ECDL_TOTAL_NUMBER_OF_DISKS_OFS, 1);

            let written = self.write_data(self.m_archive_size, &loc)?;
            if written != MZ_ZIP64_END_OF_CENTRAL_DIR_LOCATOR_SIZE {
                return Err(MzZipError::FileWriteFailed);
            }
            self.m_archive_size += MZ_ZIP64_END_OF_CENTRAL_DIR_LOCATOR_SIZE as u64;
        }

        // Write end of central directory record
        let mut eocd = [0u8; MZ_ZIP_END_OF_CENTRAL_DIR_HEADER_SIZE];
        write_le32(&mut eocd, MZ_ZIP_ECDH_SIG_OFS, MZ_ZIP_END_OF_CENTRAL_DIR_HEADER_SIG as u32);
        let clamped_files = self.m_total_files.min(0xFFFF) as u16;
        write_le16(&mut eocd, MZ_ZIP_ECDH_CDIR_NUM_ENTRIES_ON_DISK_OFS, clamped_files);
        write_le16(&mut eocd, MZ_ZIP_ECDH_CDIR_TOTAL_ENTRIES_OFS, clamped_files);
        let clamped_size = central_dir_size.min(0xFFFFFFFF) as u32;
        write_le32(&mut eocd, MZ_ZIP_ECDH_CDIR_SIZE_OFS, clamped_size);
        let clamped_ofs = central_dir_ofs.min(0xFFFFFFFF) as u32;
        write_le32(&mut eocd, MZ_ZIP_ECDH_CDIR_OFS_OFS, clamped_ofs);

        let written = self.write_data(self.m_archive_size, &eocd)?;
        if written != MZ_ZIP_END_OF_CENTRAL_DIR_HEADER_SIZE {
            return Err(MzZipError::FileWriteFailed);
        }
        self.m_archive_size += MZ_ZIP_END_OF_CENTRAL_DIR_HEADER_SIZE as u64;

        self.m_zip_mode = MzZipMode::WritingHasBeenFinalized;
        Ok(())
    }

    /// Finalize a heap-based archive and return the data.
    pub fn mz_zip_writer_finalize_heap_archive(
        &mut self,
    ) -> Result<(Vec<u8>, usize), MzZipError> {
        self.mz_zip_writer_finalize_archive()?;
        let state = self.m_pstate.as_mut().ok_or(MzZipError::InternalError)?;
        let data = state.pmem.take().unwrap_or_default();
        let size = state.mem_size;
        state.mem_size = 0;
        state.mem_capacity = 0;
        Ok((data, size))
    }

    /// End the writer and free resources.
    pub fn mz_zip_writer_end_pub(&mut self) -> bool {
        let mode = self.m_zip_mode;
        if mode != MzZipMode::Writing && mode != MzZipMode::WritingHasBeenFinalized {
            self.m_last_error = MzZipError::InvalidParameter;
            return false;
        }
        if let Some(mut state) = self.m_pstate.take() {
            state.central_dir.clear();
            state.central_dir_offsets.clear();
            state.sorted_central_dir_offsets.clear();
            state.pfile = None;
            state.pmem = None;
        } else {
            self.m_last_error = MzZipError::InvalidParameter;
            return false;
        }
        self.m_zip_mode = MzZipMode::Invalid;
        true
    }
}

// --- Writer: add_read_buf_callback ---

impl MzZipArchive {
    /// Add a file using a read callback for data.
    pub fn mz_zip_writer_add_read_buf_callback(
        &mut self,
        archive_name: &str,
        read_callback: fn(&[u8], u64, &mut [u8]) -> usize,
        callback_opaque: &[u8],
        max_size: u64,
        level_and_flags: u32,
    ) -> Result<(), MzZipError> {
        if self.m_zip_mode != MzZipMode::Writing || archive_name.is_empty() {
            return Err(MzZipError::InvalidParameter);
        }
        // Store uncompressed for now (level 0)
        let padding = self.mz_zip_writer_compute_padding_needed_for_file_alignment();
        if padding > 0 {
            let ofs = self.m_archive_size;
            self.mz_zip_writer_write_zeros(ofs, padding)?;
            self.m_archive_size += padding as u64;
        }

        let local_dir_header_ofs = self.m_archive_size;

        // Create local header
        let mut lh = [0u8; MZ_ZIP_LOCAL_DIR_HEADER_SIZE];
        mz_zip_writer_create_local_dir_header(
            self, &mut lh, archive_name.len() as u16, 0,
            max_size, max_size, 0, 0, 0, 0, 0,
        );
        let written = self.write_data(self.m_archive_size, &lh)?;
        if written != MZ_ZIP_LOCAL_DIR_HEADER_SIZE {
            return Err(MzZipError::FileWriteFailed);
        }
        self.m_archive_size += written as u64;

        // Write filename
        let written = self.write_data(self.m_archive_size, archive_name.as_bytes())?;
        self.m_archive_size += written as u64;

        // Read and write data
        let mut buf = vec![0u8; MZ_ZIP_MAX_IO_BUF_SIZE];
        let mut total_read = 0u64;
        let mut crc = 0u32;
        loop {
            let n = read_callback(callback_opaque, total_read, &mut buf);
            if n == 0 { break; }
            crc = mz_crc32(crc as u64, &buf[..n]);
            let written = self.write_data(self.m_archive_size, &buf[..n])?;
            if written != n {
                return Err(MzZipError::FileWriteFailed);
            }
            self.m_archive_size += written as u64;
            total_read += n as u64;
        }

        // Add to central directory
        self.mz_zip_writer_add_to_central_dir(
            archive_name, archive_name.len() as u16,
            &[], 0, &[], 0,
            total_read, total_read, crc, 0, 0, 0, 0,
            local_dir_header_ofs, 0, &[], 0,
        )?;
        self.m_total_files += 1;
        Ok(())
    }
}

// --- Writer: add_file, add_cfile ---

impl MzZipArchive {
    /// Add a file from disk to the archive.
    pub fn mz_zip_writer_add_file(
        &mut self,
        archive_name: &str,
        src_filename: &str,
        level_and_flags: u32,
    ) -> Result<(), MzZipError> {
        let data = std::fs::read(src_filename).map_err(|_| MzZipError::FileOpenFailed)?;
        self.mz_zip_writer_add_mem(archive_name, &data, level_and_flags)
    }
}

// --- Convenience: add_mem_to_archive_file_in_place ---

/// Convenience: append a memory blob to a ZIP file on disk (non-atomic).
pub fn mz_zip_add_mem_to_archive_file_in_place(
    zip_filename: &str,
    archive_name: &str,
    buf: &[u8],
    level_and_flags: u32,
) -> Result<(), MzZipError> {
    mz_zip_add_mem_to_archive_file_in_place_v2(zip_filename, archive_name, buf, level_and_flags)
}

/// Convenience v2: append a memory blob to a ZIP file on disk.
pub fn mz_zip_add_mem_to_archive_file_in_place_v2(
    zip_filename: &str,
    archive_name: &str,
    buf: &[u8],
    level_and_flags: u32,
) -> Result<(), MzZipError> {
    if archive_name.is_empty() {
        return Err(MzZipError::InvalidParameter);
    }
    if !mz_zip_writer_validate_archive_name(archive_name) {
        return Err(MzZipError::InvalidFilename);
    }

    let file_exists = Path::new(zip_filename).exists();
    let mut zip = MzZipArchive::new();

    if !file_exists {
        zip.mz_zip_writer_init_file_v2(zip_filename, 0, level_and_flags)?;
    } else {
        // Read existing archive
        mz_zip_reader_init_file(&mut zip, zip_filename,
            level_and_flags | MZ_ZIP_FLAG_DO_NOT_SORT_CENTRAL_DIRECTORY as u32)?;
        // Switch to writer
        zip.mz_zip_writer_init_from_reader(None)?;
    }

    let add_result = zip.mz_zip_writer_add_mem(archive_name, buf, level_and_flags);
    if let Err(ref e) = add_result {
        let _ = zip.mz_zip_writer_end_pub();
        if !file_exists {
            let _ = std::fs::remove_file(zip_filename);
        }
        return Err(*e);
    }

    if let Err(e) = zip.mz_zip_writer_finalize_archive() {
        let _ = zip.mz_zip_writer_end_pub();
        if !file_exists {
            let _ = std::fs::remove_file(zip_filename);
        }
        return Err(e);
    }

    zip.mz_zip_writer_end_pub();
    Ok(())
}

// --- Convenience: extract_archive_file_to_heap ---

/// Read a single file from a ZIP file on disk into a heap buffer.
pub fn mz_zip_extract_archive_file_to_heap(
    zip_filename: &str,
    archive_name: &str,
    flags: u32,
) -> Result<Vec<u8>, MzZipError> {
    mz_zip_extract_archive_file_to_heap_v2(zip_filename, archive_name, None, flags)
}

/// Read a single file from a ZIP file on disk into a heap buffer (v2 with comment filter).
pub fn mz_zip_extract_archive_file_to_heap_v2(
    zip_filename: &str,
    archive_name: &str,
    comment: Option<&str>,
    flags: u32,
) -> Result<Vec<u8>, MzZipError> {
    if zip_filename.is_empty() || archive_name.is_empty() {
        return Err(MzZipError::InvalidParameter);
    }
    let mut zip = MzZipArchive::new();
    mz_zip_reader_init_file(&mut zip, zip_filename,
        flags | MZ_ZIP_FLAG_DO_NOT_SORT_CENTRAL_DIRECTORY as u32)?;

    let file_index = mz_zip_reader_locate_file_v2(&zip, archive_name, comment, flags)
        .ok_or(MzZipError::FileNotFound)?;

    let result = mz_zip_reader_extract_to_heap(&zip, file_index, flags);
    mz_zip_reader_end_internal(&mut zip, result.is_ok());
    result
}

// --- CRC32 callback for validation ---

/// Callback that accumulates CRC32 over data chunks.
pub fn mz_zip_compute_crc32_callback(
    crc: &mut u32,
    _file_ofs: u64,
    buf: &[u8],
) -> usize {
    *crc = mz_crc32(*crc as u64, buf);
    buf.len()
}

// --- MzZipArray: push_bytes helper ---

impl MzZipArray {
    /// Push a slice of raw bytes into the array.
    pub fn push_bytes(&mut self, data: &[u8]) -> Result<(), MzZipError> {
        if let Some(ref mut vec) = self.p {
            vec.extend_from_slice(data);
            self.size = vec.len();
            self.capacity = vec.capacity();
            Ok(())
        } else {
            self.p = Some(data.to_vec());
            self.size = data.len();
            self.capacity = data.len();
            Ok(())
        }
    }

    /// Resize the array (truncate or extend with zeros).
    pub fn resize(&mut self, new_size: usize) {
        if let Some(ref mut vec) = self.p {
            vec.resize(new_size, 0);
            self.size = new_size;
            self.capacity = vec.capacity();
        }
    }
}

// --- MzZipInternalState: new() constructor ---

impl MzZipInternalState {
    /// Create a new default internal state.
    pub fn new() -> Self {
        Self {
            central_dir: MzZipArray::new(1),
            central_dir_offsets: MzZipArray::new(4),
            sorted_central_dir_offsets: MzZipArray::new(4),
            init_flags: 0,
            zip64: false,
            zip64_has_extended_info_fields: false,
            pfile: None,
            file_archive_start_ofs: 0,
            pmem: None,
            mem_size: 0,
            mem_capacity: 0,
        }
    }
}

impl Default for MzZipInternalState {
    fn default() -> Self {
        Self::new()
    }
}

// --- MzZipArchive: read_data method (immutable self version) ---

impl MzZipArchive {
    /// Read data from the archive backing store (memory or file) with immutable self.
    pub fn read_data_immut(&self, offset: u64, buf: &mut [u8]) -> Result<usize, MzZipError> {
        let state = self.m_pstate.as_ref().ok_or(MzZipError::InternalError)?;
        if let Some(ref pmem) = state.pmem {
            let start = (state.file_archive_start_ofs + offset) as usize;
            if start >= pmem.len() {
                return Ok(0);
            }
            let end = (start + buf.len()).min(pmem.len());
            let n = end - start;
            buf[..n].copy_from_slice(&pmem[start..end]);
            Ok(n)
        } else {
            Err(MzZipError::InternalError)
        }
    }
}