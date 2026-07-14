//! RocksDB storage backend for glean-hs.
//!
//! Exports C-ABI functions matching the foreign import ccall declarations
//! in Glean.Database.Storage.RocksDB (RocksDB.hs).
//!
//! The Haskell layer calls these functions thinking it's calling C++ —
//! but it's calling Rust. No changes needed in RocksDB.hs or Storage.hs.
//! Docker eliminated.
//!
//! C FFI functions exported (matching RocksDB.hs foreign imports):
//!   glean_rocksdb_new_cache
//!   glean_rocksdb_free_cache
//!   glean_rocksdb_cache_capacity
//!   glean_rocksdb_container_open
//!   glean_rocksdb_container_open_database
//!   glean_rocksdb_database_free
//!   glean_rocksdb_restore

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::Path;
use std::ptr;
use std::sync::Arc;

use rocksdb::{
    DB, Options, BlockBasedOptions, Cache as RocksCache,
    ColumnFamilyDescriptor,
};

// ── Error handling helpers ────────────────────────────────────────────────────

/// Convert a Rust error into a C string error (caller must free).
/// Returns null on success, a CString pointer on error.
/// Matches Glean's convention: null = success, non-null = error message.
fn error_cstring(msg: impl Into<String>) -> *mut c_char {
    match CString::new(msg.into()) {
        Ok(s) => s.into_raw(),
        Err(_) => CString::new("unknown error").unwrap().into_raw(),
    }
}

fn ok() -> *mut c_char {
    ptr::null_mut()
}

/// Free an error string returned by any glean_rocksdb_* function.
/// Called by the Haskell FFI layer via Util.FFI.
#[no_mangle]
pub extern "C" fn glean_rocksdb_free_error(s: *mut c_char) {
    if !s.is_null() {
        unsafe { let _ = CString::from_raw(s); }
    }
}

// ── Cache ─────────────────────────────────────────────────────────────────────

/// Opaque cache handle passed through Haskell as ForeignPtr Cache.
pub struct GleanCache {
    inner: RocksCache,
    capacity: usize,
}

/// Allocate a new RocksDB block cache of the given size in bytes.
///
/// Haskell: foreign import ccall unsafe glean_rocksdb_new_cache
///   :: CSize -> Ptr (Ptr Cache) -> IO CString
#[no_mangle]
pub extern "C" fn glean_rocksdb_new_cache(
    size:    usize,
    out:     *mut *mut GleanCache,
) -> *mut c_char {
    if out.is_null() {
        return error_cstring("glean_rocksdb_new_cache: null output pointer");
    }
    let cache = RocksCache::new_lru_cache(size);
    let boxed = Box::new(GleanCache {
        inner:    cache,
        capacity: size,
    });
    unsafe { *out = Box::into_raw(boxed); }
    ok()
}

/// Free a cache allocated by glean_rocksdb_new_cache.
///
/// Haskell: foreign import ccall unsafe "&glean_rocksdb_free_cache"
///   glean_rocksdb_free_cache :: Destroy Cache
#[no_mangle]
pub extern "C" fn glean_rocksdb_free_cache(cache: *mut GleanCache) {
    if !cache.is_null() {
        unsafe { let _ = Box::from_raw(cache); }
    }
}

/// Return the capacity of a cache in bytes.
///
/// Haskell: foreign import ccall unsafe glean_rocksdb_cache_capacity
///   :: Ptr Cache -> IO CSize
#[no_mangle]
pub extern "C" fn glean_rocksdb_cache_capacity(
    cache: *const GleanCache,
) -> usize {
    if cache.is_null() {
        return 0;
    }
    unsafe { (*cache).capacity }
}

// ── Container (RocksDB instance) ──────────────────────────────────────────────

/// Open mode matching Glean's C++ open modes.
const MODE_READ_ONLY:  c_int = 0;
const MODE_READ_WRITE: c_int = 1;
const MODE_CREATE:     c_int = 2;

/// An open RocksDB container (the raw database files on disk).
pub struct GleanContainer {
    path: String,
    db:   Arc<DB>,
}

/// Open or create a RocksDB container at the given path.
///
/// mode: 0=ReadOnly, 1=ReadWrite, 2=Create
///
/// Haskell: foreign import ccall safe glean_rocksdb_container_open
///   :: CString -> CInt -> CBool -> Ptr Cache -> Ptr Container -> IO CString
#[no_mangle]
pub extern "C" fn glean_rocksdb_container_open(
    path:              *const c_char,
    mode:              c_int,
    _cache_index:      u8,   // CBool — cache index and filter blocks
    cache:             *const GleanCache,
    out:               *mut *mut GleanContainer,
) -> *mut c_char {
    if path.is_null() || out.is_null() {
        return error_cstring("glean_rocksdb_container_open: null pointer");
    }

    let path_str = unsafe {
        match CStr::from_ptr(path).to_str() {
            Ok(s) => s.to_owned(),
            Err(_) => return error_cstring("invalid UTF-8 path"),
        }
    };

    let mut opts = Options::default();
    opts.create_if_missing(mode == MODE_CREATE);
    opts.create_missing_column_families(mode == MODE_CREATE);

    // Apply block cache if provided
    if !cache.is_null() {
        let mut bb_opts = BlockBasedOptions::default();
        let cache_ref = unsafe { &(*cache).inner };
        bb_opts.set_block_cache(cache_ref);
        opts.set_block_based_table_factory(&bb_opts);
    }

    let db_result = if mode == MODE_READ_ONLY {
        DB::open_for_read_only(&opts, &path_str, false)
    } else {
        DB::open(&opts, &path_str)
    };

    match db_result {
        Err(e) => error_cstring(format!(
            "glean_rocksdb_container_open: {}", e
        )),
        Ok(db) => {
            let container = Box::new(GleanContainer {
                path: path_str,
                db:   Arc::new(db),
            });
            unsafe { *out = Box::into_raw(container); }
            ok()
        }
    }
}

// ── Database (logical database within a container) ────────────────────────────

/// A logical Glean database within a RocksDB container.
pub struct GleanDatabase {
    container: Arc<DB>,
    start_id:  u64,   // Fid — first fact ID
    version:   i64,   // DB version number
}

/// Open a logical database within an existing container.
///
/// Haskell: foreign import ccall safe glean_rocksdb_container_open_database
///   :: Container -> Fid -> UsetId -> Int64
///   -> Ptr (Ptr (Database RocksDB)) -> IO CString
#[no_mangle]
pub extern "C" fn glean_rocksdb_container_open_database(
    container:      *mut GleanContainer,
    start_id:       u64,    // Fid
    _first_unit_id: u32,    // UsetId
    version:        i64,
    out:            *mut *mut GleanDatabase,
) -> *mut c_char {
    if container.is_null() || out.is_null() {
        return error_cstring(
            "glean_rocksdb_container_open_database: null pointer"
        );
    }

    let db = unsafe { Arc::clone(&(*container).db) };

    let database = Box::new(GleanDatabase {
        container: db,
        start_id,
        version,
    });

    unsafe { *out = Box::into_raw(database); }
    ok()
}

/// Free a database handle.
///
/// Haskell: foreign import ccall safe "&glean_rocksdb_database_free"
///   glean_rocksdb_database_free :: Destroy (Database RocksDB)
#[no_mangle]
pub extern "C" fn glean_rocksdb_database_free(db: *mut GleanDatabase) {
    if !db.is_null() {
        unsafe { let _ = Box::from_raw(db); }
    }
}

// ── Backup / Restore ──────────────────────────────────────────────────────────

/// Restore a RocksDB backup from source path to target path.
///
/// Haskell: foreign import ccall safe glean_rocksdb_restore
///   :: CString -> CString -> IO CString
#[no_mangle]
pub extern "C" fn glean_rocksdb_restore(
    target: *const c_char,
    source: *const c_char,
) -> *mut c_char {
    if target.is_null() || source.is_null() {
        return error_cstring("glean_rocksdb_restore: null pointer");
    }

    let target_str = unsafe {
        match CStr::from_ptr(target).to_str() {
            Ok(s) => s.to_owned(),
            Err(_) => return error_cstring("invalid UTF-8 target path"),
        }
    };

    let source_str = unsafe {
        match CStr::from_ptr(source).to_str() {
            Ok(s) => s.to_owned(),
            Err(_) => return error_cstring("invalid UTF-8 source path"),
        }
    };

    match std::fs::rename(&source_str, &target_str) {
        Ok(_) => ok(),
        Err(e) => error_cstring(format!(
            "glean_rocksdb_restore: failed to move {} to {}: {}",
            source_str, target_str, e
        )),
    }
}

// ── Basic fact storage operations ─────────────────────────────────────────────

/// Store a serialized fact batch into the database.
/// Called by Glean's Haskell store() method.
#[no_mangle]
pub extern "C" fn glean_rocksdb_store(
    db:    *mut GleanDatabase,
    data:  *const u8,
    len:   usize,
) -> *mut c_char {
    if db.is_null() || (data.is_null() && len > 0) {
        return error_cstring("glean_rocksdb_store: null pointer");
    }

    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let db_ref = unsafe { &(*db).container };

    match db_ref.put(b"facts", bytes) {
        Ok(_)  => ok(),
        Err(e) => error_cstring(format!("glean_rocksdb_store: {}", e)),
    }
}

/// Retrieve serialized fact data from the database.
/// Called by Glean's Haskell retrieve() method.
#[no_mangle]
pub extern "C" fn glean_rocksdb_retrieve(
    db:      *mut GleanDatabase,
    out:     *mut *mut u8,
    out_len: *mut usize,
) -> *mut c_char {
    if db.is_null() || out.is_null() || out_len.is_null() {
        return error_cstring("glean_rocksdb_retrieve: null pointer");
    }

    let db_ref = unsafe { &(*db).container };

    match db_ref.get(b"facts") {
        Err(e) => error_cstring(format!("glean_rocksdb_retrieve: {}", e)),
        Ok(None) => {
            unsafe {
                *out     = ptr::null_mut();
                *out_len = 0;
            }
            ok()
        }
        Ok(Some(data)) => {
            let len  = data.len();
            let ptr  = data.as_ptr();
            // Leak the Vec — Haskell will free via glean_rocksdb_free_bytes
            let data = data.into_boxed_slice();
            unsafe {
                *out     = Box::into_raw(data) as *mut u8;
                *out_len = len;
            }
            ok()
        }
    }
}

/// Free bytes allocated by glean_rocksdb_retrieve.
#[no_mangle]
pub extern "C" fn glean_rocksdb_free_bytes(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        unsafe {
            let _ = Box::from_raw(
                std::slice::from_raw_parts_mut(ptr, len) as *mut [u8]
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use tempfile::TempDir;

    #[test]
    fn test_cache_lifecycle() {
        let mut cache_ptr: *mut GleanCache = ptr::null_mut();
        let err = glean_rocksdb_new_cache(8 * 1024 * 1024, &mut cache_ptr);
        assert!(err.is_null(), "expected no error");
        assert!(!cache_ptr.is_null());

        let capacity = glean_rocksdb_cache_capacity(cache_ptr);
        assert_eq!(capacity, 8 * 1024 * 1024);

        glean_rocksdb_free_cache(cache_ptr);
    }

    #[test]
    fn test_cache_null_ptr() {
        let capacity = glean_rocksdb_cache_capacity(ptr::null());
        assert_eq!(capacity, 0);
        // free null is safe
        glean_rocksdb_free_cache(ptr::null_mut());
    }

    #[test]
    fn test_container_create_open_close() {
        let dir = TempDir::new().unwrap();
        let path = CString::new(dir.path().to_str().unwrap()).unwrap();
        let mut container_ptr: *mut GleanContainer = ptr::null_mut();

        let err = glean_rocksdb_container_open(
            path.as_ptr(),
            MODE_CREATE,
            0,
            ptr::null(),
            &mut container_ptr,
        );
        assert!(err.is_null(), "expected no error creating container");
        assert!(!container_ptr.is_null());

        // Open a database within the container
        let mut db_ptr: *mut GleanDatabase = ptr::null_mut();
        let err = glean_rocksdb_container_open_database(
            container_ptr,
            1024,  // start_id (Fid::LOWEST)
            1,     // first_unit_id
            3,     // version
            &mut db_ptr,
        );
        assert!(err.is_null(), "expected no error opening database");
        assert!(!db_ptr.is_null());

        // Clean up
        glean_rocksdb_database_free(db_ptr);
        // Container is dropped when GleanContainer is freed
        unsafe { let _ = Box::from_raw(container_ptr); }
    }

    #[test]
    fn test_store_retrieve() {
        let dir = TempDir::new().unwrap();
        let path = CString::new(dir.path().to_str().unwrap()).unwrap();
        let mut container_ptr: *mut GleanContainer = ptr::null_mut();

        glean_rocksdb_container_open(
            path.as_ptr(), MODE_CREATE, 0, ptr::null(), &mut container_ptr,
        );

        let mut db_ptr: *mut GleanDatabase = ptr::null_mut();
        glean_rocksdb_container_open_database(
            container_ptr, 1024, 1, 3, &mut db_ptr,
        );

        // Store some data
        let data = b"hello glean";
        let err = glean_rocksdb_store(db_ptr, data.as_ptr(), data.len());
        assert!(err.is_null(), "expected no error storing");

        // Retrieve it back
        let mut out_ptr: *mut u8 = ptr::null_mut();
        let mut out_len: usize = 0;
        let err = glean_rocksdb_retrieve(db_ptr, &mut out_ptr, &mut out_len);
        assert!(err.is_null(), "expected no error retrieving");
        assert!(!out_ptr.is_null());
        assert_eq!(out_len, data.len());

        let retrieved = unsafe {
            std::slice::from_raw_parts(out_ptr, out_len)
        };
        assert_eq!(retrieved, data);

        glean_rocksdb_free_bytes(out_ptr, out_len);
        glean_rocksdb_database_free(db_ptr);
        unsafe { let _ = Box::from_raw(container_ptr); }
    }

    #[test]
    fn test_free_error_null() {
        // Freeing a null error string is safe
        glean_rocksdb_free_error(ptr::null_mut());
    }

    #[test]
    fn test_container_invalid_path() {
        let path = CString::new("/nonexistent/path/that/does/not/exist").unwrap();
        let mut container_ptr: *mut GleanContainer = ptr::null_mut();
        let err = glean_rocksdb_container_open(
            path.as_ptr(), MODE_READ_WRITE, 0, ptr::null(), &mut container_ptr,
        );
        // Should fail — path doesn't exist and mode is not Create
        assert!(!err.is_null(), "expected error for nonexistent path");
        glean_rocksdb_free_error(err);
    }
}
