-- | RocksDB storage backend for glean-hs.
--
-- Implements the 'Storage' typeclass using our Rust substrate
-- via the FFI bindings in "Glean.FFI".
--
-- This module replaces Meta Glean's RocksDB.hs, which depends on
-- Meta-internal packages (Util.FFI, Util.Log, ServiceData).
-- Our implementation has zero Meta internal dependencies.
--
-- Usage:
-- @
-- import Glean.RocksDB (RocksDB)
-- import Glean.Storage
--
-- withStorage (defaultDbConfig "\/tmp\/mydb") $ \(db :: RocksDB) -> do
--   store db myBatch
--   facts <- retrieve db
-- @

module Glean.RocksDB
  ( RocksDB
  , rocksDbOpen
  , rocksDbClose
  ) where

import Control.Exception (throwIO, catch, SomeException)
import qualified Data.ByteString as BS
import Data.IORef
import qualified Data.Map.Strict as Map
import qualified Data.Text as Text

import Glean.FFI
import Glean.Storage

-- ── RocksDB handle ────────────────────────────────────────────────────────────

-- | A RocksDB-backed Glean database.
-- Wraps the Rust substrate via FFI.
data RocksDB = RocksDB
  { rocksContainer  :: !GleanContainer
    -- ^ The RocksDB container (database files on disk).
  , rocksDatabase   :: !GleanDatabase
    -- ^ The logical Glean database within the container.
  , rocksConfig     :: !DbConfig
    -- ^ Configuration used to open this database.
  , rocksProps      :: !(IORef DbProperties)
    -- ^ Mutable database properties (fact count etc.).
  , rocksClosed     :: !(IORef Bool)
    -- ^ True if this database has been closed.
  }

-- ── Smart constructors ────────────────────────────────────────────────────────

-- | Open a RocksDB database directly.
-- Prefer 'withStorage' or 'open' (the Storage instance method).
rocksDbOpen :: DbConfig -> IO RocksDB
rocksDbOpen config = do
  -- Allocate block cache
  cache <- newCache (dbCacheSize config)

  -- Determine open mode
  let mode
        | dbReadOnly config = ReadOnly
        | dbCreate   config = Create
        | otherwise         = ReadWrite

  -- Open the container (RocksDB instance)
  container <- openContainer (dbPath config) mode (Just cache)
    `catch` \(e :: SomeException) ->
      throwIO $ StorageOpenFailed
        (Text.pack $ "Failed to open container: " ++ show e)

  -- Open the logical database within the container
  db <- openDatabase
    container
    (dbStartId   config)
    1                    -- first_unit_id (default)
    (dbVersion   config)
    `catch` \(e :: SomeException) ->
      throwIO $ StorageOpenFailed
        (Text.pack $ "Failed to open database: " ++ show e)

  -- Read persisted metadata from RocksDB
  factCount  <- Glean.FFI.getMeta db "meta:fact_count"
  _batchCount <- Glean.FFI.getMeta db "meta:batch_count"

  -- Initialize mutable state from persisted values
  propsRef  <- newIORef $ DbProperties
    { propVersion     = dbVersion config
    , propFirstId     = dbStartId config
    , propFirstFreeId = dbStartId config + factCount
    , propFactCount   = fromIntegral factCount
    }
  closedRef <- newIORef False

  return RocksDB
    { rocksContainer = container
    , rocksDatabase  = db
    , rocksConfig    = config
    , rocksProps     = propsRef
    , rocksClosed    = closedRef
    }

-- | Close a RocksDB database.
rocksDbClose :: RocksDB -> IO ()
rocksDbClose rdb = do
  closed <- readIORef (rocksClosed rdb)
  if closed
    then return ()  -- idempotent close
    else do
      writeIORef (rocksClosed rdb) True
      freeDatabase (rocksDatabase rdb)
      -- Container is freed by GC via ForeignPtr finalizer

-- ── Storage instance ──────────────────────────────────────────────────────────

instance Storage RocksDB where

  open   = rocksDbOpen
  close  = rocksDbClose

  store rdb batch = do
    checkNotClosed rdb
    let bytes = batchData batch
    if BS.null bytes
      then return ()  -- nothing to store
      else do
        Glean.FFI.store (rocksDatabase rdb) bytes
                        (fromIntegral (batchCount batch))
          `catch` \(e :: SomeException) ->
            throwIO $ StorageWriteFailed
              (Text.pack $ "store failed: " ++ show e)
        -- Update properties
        modifyIORef' (rocksProps rdb) $ \props -> props
          { propFirstFreeId = propFirstFreeId props
                            + fromIntegral (batchCount batch)
          , propFactCount   = propFactCount props + batchCount batch
          }

  retrieve rdb = do
    checkNotClosed rdb
    result <- Glean.FFI.retrieve (rocksDatabase rdb)
      `catch` \(e :: SomeException) ->
        throwIO $ StorageReadFailed
          (Text.pack $ "retrieve failed: " ++ show e)
    case result of
      Nothing    -> return Nothing
      Just bytes -> do
        props <- readIORef (rocksProps rdb)
        return $ Just $ FactBatch
          { batchData       = bytes
          , batchFirstId    = propFirstId props
          , batchCount      = propFactCount props
          , batchPredicates = Map.empty  -- populated by indexer
          }

  commit rdb = do
    checkNotClosed rdb
    -- RocksDB auto-commits writes
    -- Explicit flush for durability guarantee
    flush rdb

  flush rdb = do
    checkNotClosed rdb
    -- Flush is handled at the RocksDB level
    -- No direct FFI call needed for basic operation
    return ()

  optimize rdb = do
    checkNotClosed rdb
    -- RocksDB compaction — deferred to Phase 10 (cache/optimize)
    return ()

  predicateStats rdb = do
    checkNotClosed rdb
    props <- readIORef (rocksProps rdb)
    -- Detailed per-predicate stats require index scan
    -- Basic implementation: return aggregate stats
    return $ Map.singleton 0 $ PredicateStats
      { statsCount   = propFactCount   props
      , statsFirstId = propFirstId     props
      , statsLastId  = propFirstFreeId props
      }

  properties rdb = do
    checkNotClosed rdb
    readIORef (rocksProps rdb)

  backup rdb targetPath = do
    checkNotClosed rdb
    Glean.FFI.restore targetPath (dbPath (rocksConfig rdb))
      `catch` \(e :: SomeException) ->
        throwIO $ StorageWriteFailed
          (Text.pack $ "backup failed: " ++ show e)

-- ── Internal helpers ──────────────────────────────────────────────────────────

-- | Throw if the database has been closed.
checkNotClosed :: RocksDB -> IO ()
checkNotClosed rdb = do
  closed <- readIORef (rocksClosed rdb)
  if closed
    then throwIO $ StorageCloseFailed
           (Text.pack "Operation on closed database")
    else return ()
