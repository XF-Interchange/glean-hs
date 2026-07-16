-- | Foreign function interface to the glean-hs Rust substrate.
--
-- Binds the C-ABI functions exported by src/storage/rocksdb.rs.
-- The Rust library must be built before this module can link:
--
-- @
-- cargo build --release
-- cabal build
-- @
--
-- Error handling convention (matching Meta Glean):
--   Every function returns a CString error message.
--   Null pointer = success.
--   Non-null pointer = error message (must be freed via freeError).

module Glean.FFI
  ( -- * Cache
    GleanCache
  , newCache
  , freeCache
  , cacheCapacity

    -- * Container
  , GleanContainer
  , OpenMode (..)
  , openContainer

    -- * Database
  , GleanDatabase
  , openDatabase
  , freeDatabase

    -- * Backup/Restore
  , restore

    -- * Fact storage
  , store
  , retrieve
  , freeBytes
  , getMeta

    -- * Error handling
  , GleanError
  , checkError
  ) where

import Control.Exception (throwIO, Exception)
import Data.ByteString (ByteString)
import qualified Data.ByteString as BS
import qualified Data.ByteString.Unsafe as BSU
import Foreign
import Foreign.C.String
import Foreign.C.Types

-- ── Opaque types ──────────────────────────────────────────────────────────────

-- | Opaque handle to a RocksDB block cache.
data CCache
-- | Opaque handle to a RocksDB container (database files on disk).
data CContainer
-- | Opaque handle to a logical Glean database.
data CDatabase

-- | Type aliases for clarity at the Haskell level.
type GleanCache     = ForeignPtr CCache
type GleanContainer = ForeignPtr CContainer
type GleanDatabase  = ForeignPtr CDatabase

-- ── Error handling ────────────────────────────────────────────────────────────

-- | An error returned by the Rust substrate.
newtype GleanError = GleanError String
  deriving (Show)

instance Exception GleanError

-- | Free an error string returned by the Rust substrate.
foreign import ccall unsafe "glean_rocksdb_free_error"
  c_free_error :: CString -> IO ()

-- | Check a C error string. Null = success; non-null = throw GleanError.
checkError :: CString -> IO ()
checkError ptr
  | ptr == nullPtr = return ()
  | otherwise = do
      msg <- peekCString ptr
      c_free_error ptr
      throwIO (GleanError msg)

-- ── Cache FFI ─────────────────────────────────────────────────────────────────

foreign import ccall unsafe "glean_rocksdb_new_cache"
  c_new_cache :: CSize -> Ptr (Ptr CCache) -> IO CString

foreign import ccall unsafe "&glean_rocksdb_free_cache"
  c_free_cache :: FinalizerPtr CCache

foreign import ccall unsafe "glean_rocksdb_cache_capacity"
  c_cache_capacity :: Ptr CCache -> IO CSize

-- | Allocate a new RocksDB block cache of the given size in bytes.
newCache :: Int -> IO GleanCache
newCache size =
  alloca $ \pptr -> do
    err <- c_new_cache (fromIntegral size) pptr
    checkError err
    ptr <- peek pptr
    newForeignPtr c_free_cache ptr

-- | Free a cache (called automatically by GC via ForeignPtr finalizer).
freeCache :: GleanCache -> IO ()
freeCache = finalizeForeignPtr

-- | Return the capacity of a cache in bytes.
cacheCapacity :: GleanCache -> IO Int
cacheCapacity cache =
  withForeignPtr cache $ \ptr ->
    fromIntegral <$> c_cache_capacity ptr

-- ── Container FFI ─────────────────────────────────────────────────────────────

foreign import ccall safe "glean_rocksdb_container_open"
  c_container_open
    :: CString    -- path
    -> CInt       -- mode (0=ReadOnly, 1=ReadWrite, 2=Create)
    -> Word8      -- cache_index (CBool)
    -> Ptr CCache -- cache (nullable)
    -> Ptr (Ptr CContainer)
    -> IO CString

-- | How to open a RocksDB container.
data OpenMode
  = ReadOnly
  | ReadWrite
  | Create
  deriving (Show, Eq)

openModeToInt :: OpenMode -> CInt
openModeToInt ReadOnly  = 0
openModeToInt ReadWrite = 1
openModeToInt Create    = 2

-- | Open or create a RocksDB container at the given path.
openContainer
  :: FilePath
  -> OpenMode
  -> Maybe GleanCache  -- ^ Optional block cache
  -> IO GleanContainer
openContainer path mode mCache =
  withCString path $ \cpath ->
  alloca $ \pptr -> do
    err <- case mCache of
      Nothing ->
        c_container_open cpath (openModeToInt mode) 0 nullPtr pptr
      Just cache ->
        withForeignPtr cache $ \cptr ->
          c_container_open cpath (openModeToInt mode) 1 cptr pptr
    checkError err
    ptr <- peek pptr
    newForeignPtr_ ptr  -- no finalizer yet — container lifetime managed manually

-- ── Database FFI ──────────────────────────────────────────────────────────────

foreign import ccall safe "glean_rocksdb_container_open_database"
  c_open_database
    :: Ptr CContainer
    -> Word64   -- start_id (Fid)
    -> Word32   -- first_unit_id (UsetId)
    -> Int64    -- version
    -> Ptr (Ptr CDatabase)
    -> IO CString

foreign import ccall safe "&glean_rocksdb_database_free"
  c_free_database :: FinalizerPtr CDatabase

-- | Open a logical database within a container.
openDatabase
  :: GleanContainer
  -> Word64   -- ^ Starting fact ID (Fid::LOWEST = 1024)
  -> Word32   -- ^ First unit ID
  -> Int64    -- ^ Schema version
  -> IO GleanDatabase
openDatabase container startId firstUnitId version =
  withForeignPtr container $ \cptr ->
  alloca $ \pptr -> do
    err <- c_open_database cptr startId firstUnitId version pptr
    checkError err
    ptr <- peek pptr
    newForeignPtr c_free_database ptr

-- | Free a database handle.
freeDatabase :: GleanDatabase -> IO ()
freeDatabase = finalizeForeignPtr

-- ── Backup/Restore FFI ────────────────────────────────────────────────────────

foreign import ccall safe "glean_rocksdb_restore"
  c_restore :: CString -> CString -> IO CString

-- | Restore a database from source path to target path.
restore :: FilePath -> FilePath -> IO ()
restore target source =
  withCString target $ \ctarget ->
  withCString source $ \csource -> do
    err <- c_restore ctarget csource
    checkError err

-- ── Fact storage FFI ──────────────────────────────────────────────────────────

foreign import ccall safe "glean_rocksdb_store"
  c_store :: Ptr CDatabase -> Ptr Word8 -> CSize -> Word64 -> IO CString

foreign import ccall safe "glean_rocksdb_retrieve"
  c_retrieve :: Ptr CDatabase -> Ptr (Ptr Word8) -> Ptr CSize -> IO CString

foreign import ccall unsafe "glean_rocksdb_free_bytes"
  c_free_bytes :: Ptr Word8 -> CSize -> IO ()

-- | Store a serialized fact batch into the database.
-- fact_count is the exact number of facts in this batch.
store :: GleanDatabase -> ByteString -> Word64 -> IO ()
store db bytes factCount =
  withForeignPtr db $ \dbptr ->
  BSU.unsafeUseAsCStringLen bytes $ \(ptr, len) -> do
    err <- c_store dbptr (castPtr ptr) (fromIntegral len) factCount
    checkError err

-- | Retrieve serialized fact data from the database.
-- Returns Nothing if no facts are stored yet.
retrieve :: GleanDatabase -> IO (Maybe ByteString)
retrieve db =
  withForeignPtr db $ \dbptr ->
  alloca $ \pptr ->
  alloca $ \plen -> do
    err <- c_retrieve dbptr pptr plen
    checkError err
    ptr <- peek pptr
    if ptr == nullPtr
      then return Nothing
      else do
        len <- peek plen
        bs  <- BS.packCStringLen (castPtr ptr, fromIntegral len)
        c_free_bytes ptr len
        return (Just bs)

-- | Free bytes allocated by retrieve (called internally).
freeBytes :: Ptr Word8 -> Int -> IO ()
freeBytes ptr len = c_free_bytes ptr (fromIntegral len)

-- ── Metadata FFI ──────────────────────────────────────────────────────────────

foreign import ccall safe "glean_rocksdb_get_meta"
  c_get_meta :: Ptr CDatabase -> CString -> Ptr Word64 -> IO CString

-- | Read a u64 metadata value by key from the database.
-- Returns 0 if the key does not exist.
getMeta :: GleanDatabase -> String -> IO Word64
getMeta db key =
  withForeignPtr db $ \dbptr ->
  withCString key $ \ckey ->
  alloca $ \pval -> do
    err <- c_get_meta dbptr ckey pval
    checkError err
    peek pval
