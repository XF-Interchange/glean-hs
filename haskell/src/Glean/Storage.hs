-- | Storage typeclass for glean-hs.
--
-- A clean, minimal reimplementation of Glean's Storage abstraction.
-- No Meta internal dependencies (no ODS, ServiceData, Util.FFI).
--
-- The Storage typeclass defines the interface between the Glean
-- Haskell layer and the underlying database backend (RocksDB).
--
-- Implementations:
--   Glean.RocksDB   — production storage via Rust substrate
--   Glean.Memory    — in-memory storage for testing (future)

module Glean.Storage
  ( -- * Storage typeclass
    Storage (..)

    -- * Database configuration
  , DbConfig (..)
  , defaultDbConfig

    -- * Fact batch
  , FactBatch (..)
  , emptyBatch
  , batchSize

    -- * Predicate statistics
  , PredicateStats (..)
  , emptyStats

    -- * Database properties
  , DbProperties (..)

    -- * Errors
  , StorageError (..)

    -- * Utilities
  , withStorage
  ) where

import Control.Exception (Exception, bracket, throwIO)
import Data.ByteString (ByteString)
import qualified Data.ByteString as BS
import Data.Int (Int64)
import Data.Map.Strict (Map)
import qualified Data.Map.Strict as Map
import Data.Text (Text)
import qualified Data.Text as Text
import Data.Word (Word32, Word64)

-- ── Errors ────────────────────────────────────────────────────────────────────

-- | Errors that can occur during storage operations.
data StorageError
  = StorageOpenFailed Text     -- ^ Failed to open database
  | StorageWriteFailed Text    -- ^ Failed to write facts
  | StorageReadFailed Text     -- ^ Failed to read facts
  | StorageCloseFailed Text    -- ^ Failed to close database
  | StorageCorrupted Text      -- ^ Database corruption detected
  | StorageVersionMismatch
      { expected :: Int64
      , actual   :: Int64
      }                        -- ^ Schema version mismatch
  deriving (Show)

instance Exception StorageError

-- ── Configuration ─────────────────────────────────────────────────────────────

-- | Configuration for opening a database.
data DbConfig = DbConfig
  { dbPath        :: FilePath
    -- ^ Path to the database directory on disk.
  , dbReadOnly    :: Bool
    -- ^ Open in read-only mode (no writes allowed).
  , dbCreate      :: Bool
    -- ^ Create the database if it doesn't exist.
  , dbCacheSize   :: Int
    -- ^ RocksDB block cache size in bytes (default: 128MB).
  , dbStartId     :: Word64
    -- ^ Starting fact ID. Use 1024 (Fid::LOWEST) for new databases.
  , dbVersion     :: Int64
    -- ^ Schema version number.
  } deriving (Show, Eq)

-- | Sensible defaults for a new read-write database.
defaultDbConfig :: FilePath -> DbConfig
defaultDbConfig path = DbConfig
  { dbPath      = path
  , dbReadOnly  = False
  , dbCreate    = True
  , dbCacheSize = 128 * 1024 * 1024  -- 128MB
  , dbStartId   = 1024               -- Fid::LOWEST
  , dbVersion   = 1
  }

-- ── Fact batch ────────────────────────────────────────────────────────────────

-- | A batch of serialized facts ready to store.
-- Facts are encoded using Glean's binary format (nat.rs, binary.rs).
data FactBatch = FactBatch
  { batchData      :: !ByteString
    -- ^ Binary-encoded fact data.
  , batchFirstId   :: !Word64
    -- ^ The fact ID of the first fact in this batch.
  , batchCount     :: !Int
    -- ^ Number of facts in this batch.
  , batchPredicates :: !(Map Word64 Int)
    -- ^ Map from predicate ID (Pid) to count of facts for that predicate.
  } deriving (Show, Eq)

-- | An empty fact batch.
emptyBatch :: Word64 -> FactBatch
emptyBatch firstId = FactBatch
  { batchData       = BS.empty
  , batchFirstId    = firstId
  , batchCount      = 0
  , batchPredicates = Map.empty
  }

-- | Total number of facts in a batch.
batchSize :: FactBatch -> Int
batchSize = batchCount

-- ── Predicate statistics ──────────────────────────────────────────────────────

-- | Statistics for a single predicate.
data PredicateStats = PredicateStats
  { statsCount     :: !Int
    -- ^ Number of facts for this predicate.
  , statsFirstId   :: !Word64
    -- ^ First fact ID for this predicate.
  , statsLastId    :: !Word64
    -- ^ Last fact ID for this predicate.
  } deriving (Show, Eq)

-- | Empty predicate statistics.
emptyStats :: PredicateStats
emptyStats = PredicateStats
  { statsCount   = 0
  , statsFirstId = 0
  , statsLastId  = 0
  }

-- ── Database properties ───────────────────────────────────────────────────────

-- | Properties of an open database.
data DbProperties = DbProperties
  { propVersion    :: !Int64
    -- ^ Schema version number.
  , propFirstId    :: !Word64
    -- ^ First fact ID in the database.
  , propFirstFreeId :: !Word64
    -- ^ Next available fact ID.
  , propFactCount  :: !Int
    -- ^ Total number of facts stored.
  } deriving (Show, Eq)

-- ── Storage typeclass ─────────────────────────────────────────────────────────

-- | Abstract storage backend for a Glean database.
--
-- All operations are in IO and may throw 'StorageError'.
--
-- Minimal complete definition: 'open', 'close', 'store', 'retrieve'.
class Storage s where

  -- | Open a database with the given configuration.
  -- Throws 'StorageOpenFailed' if the database cannot be opened.
  open :: DbConfig -> IO s

  -- | Close a database, flushing any pending writes.
  -- Throws 'StorageCloseFailed' if close fails.
  close :: s -> IO ()

  -- | Store a batch of facts.
  -- Throws 'StorageWriteFailed' if the write fails.
  store :: s -> FactBatch -> IO ()

  -- | Retrieve all stored facts.
  -- Returns Nothing if no facts have been stored yet.
  -- Throws 'StorageReadFailed' if the read fails.
  retrieve :: s -> IO (Maybe FactBatch)

  -- | Commit pending writes to durable storage.
  -- Default implementation: no-op (some backends auto-commit).
  commit :: s -> IO ()
  commit _ = return ()

  -- | Get statistics for all predicates.
  predicateStats :: s -> IO (Map Word64 PredicateStats)
  predicateStats _ = return Map.empty

  -- | Get database properties.
  properties :: s -> IO DbProperties
  properties _ = return $ DbProperties
    { propVersion     = 1
    , propFirstId     = 1024
    , propFirstFreeId = 1024
    , propFactCount   = 0
    }

  -- | Optimize the database (compact, etc.).
  -- Default implementation: no-op.
  optimize :: s -> IO ()
  optimize _ = return ()

  -- | Flush in-memory data to disk.
  -- Default implementation: no-op.
  flush :: s -> IO ()
  flush _ = return ()

  -- | Create a backup of the database at the given path.
  backup :: s -> FilePath -> IO ()
  backup _ _ = return ()

-- ── Utility ───────────────────────────────────────────────────────────────────

-- | Open a database, run an action, and close it safely.
-- Ensures 'close' is called even if the action throws.
--
-- Example:
-- @
-- withStorage (defaultDbConfig "\/tmp\/mydb") $ \db -> do
--   store db myBatch
--   retrieve db
-- @
withStorage :: Storage s => DbConfig -> (s -> IO a) -> IO a
withStorage config = bracket (open config) close
