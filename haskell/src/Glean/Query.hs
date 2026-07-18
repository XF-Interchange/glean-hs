-- | Direct query layer for glean-hs.
--
-- Phase 13, Step 1: batch scanning queries.
-- Correct but O(n) — sufficient for small databases.
--
-- When indexing large projects (e.g. SeidoClaims, 50K+ facts),
-- migrate storage to composite keys (encode(pid) + fact_key)
-- for O(1) point lookups. The interface here stays the same.
--
-- Usage:
-- @
-- import Glean.Query
--
-- defs <- findDefinitions db "validateCDTCode"
-- refs <- findReferences  db "validateCDTCode"
-- mods <- findByModule    db "SeidoClaims.Validation"
-- @

module Glean.Query
  ( -- * Definition queries
    findDefinitions
  , findDefinition

    -- * Reference queries
  , findReferences

    -- * Module queries
  , findByModule
  , findModules

    -- * General queries
  , findFacts
  , QueryResult (..)
  ) where

import Data.ByteString (ByteString)
import qualified Data.ByteString as BS
import Data.Maybe (mapMaybe)
import Data.Text (Text)
import qualified Data.Text as Text
import qualified Data.Text.Encoding as Text
import Data.Word (Word32, Word64)

import Glean.Storage
import Glean.Indexer.Types

-- ── Query result type ─────────────────────────────────────────────────────────

-- | A query result — a fact matching the query criteria.
data QueryResult
  = DefinitionResult DefinitionFact
  | ReferenceResult  ReferenceFact
  | ModuleResult     ModuleFact
  | ImportResult     ImportFact
  deriving (Show, Eq)

-- ── Public query API ──────────────────────────────────────────────────────────

-- | Find all definitions of a given name across all modules.
-- O(n) — scans all stored batches.
findDefinitions :: Storage s => s -> Text -> IO [DefinitionFact]
findDefinitions db name = do
  facts <- loadAllFacts db
  return $ filter (\d -> defName d == name)
         $ mapMaybe toDefinition facts

-- | Find the first definition of a given name.
-- Returns Nothing if not found.
findDefinition :: Storage s => s -> Text -> IO (Maybe DefinitionFact)
findDefinition db name = do
  defs <- findDefinitions db name
  return $ case defs of
    []    -> Nothing
    (d:_) -> Just d

-- | Find all references to a given name.
-- O(n) — scans all stored batches.
findReferences :: Storage s => s -> Text -> IO [ReferenceFact]
findReferences db name = do
  facts <- loadAllFacts db
  return $ filter (\r -> refName r == name)
         $ mapMaybe toReference facts

-- | Find all facts (definitions, references) in a given module.
-- O(n) — scans all stored batches.
findByModule :: Storage s => s -> Text -> IO [QueryResult]
findByModule db moduleName = do
  facts <- loadAllFacts db
  return $ mapMaybe (matchModule moduleName) facts

-- | Find all indexed modules.
-- O(n) — scans all stored batches.
findModules :: Storage s => s -> IO [ModuleFact]
findModules db = do
  facts <- loadAllFacts db
  return $ mapMaybe toModule facts

-- | General fact query by predicate ID and optional name filter.
findFacts :: Storage s => s -> Word64 -> Maybe Text -> IO [QueryResult]
findFacts db pid mName = do
  facts <- loadAllFacts db
  let matching = filter (matchesPid pid) facts
  let filtered = case mName of
        Nothing   -> matching
        Just name -> filter (matchesName name) matching
  return $ mapMaybe toQueryResult filtered

-- ── Batch loading ─────────────────────────────────────────────────────────────

-- | Load and deserialize all facts from the database.
-- This is the O(n) core — reads all batches and deserializes them.
-- TODO Phase 13 Step 2: replace with composite key point lookups.
loadAllFacts :: Storage s => s -> IO [RawFact]
loadAllFacts db = do
  result <- retrieve db
  case result of
    Nothing    -> do
      return []
    Just batch -> do
      let facts = deserializeBatch batch
      return facts

-- ── Raw fact type (internal) ──────────────────────────────────────────────────

-- | A deserialized raw fact from the database.
data RawFact = RawFact
  { rawPid  :: !Word64
  , rawData :: !ByteString
  } deriving (Show, Eq)

-- ── Deserialization ───────────────────────────────────────────────────────────

-- | Deserialize a FactBatch into raw facts.
deserializeBatch :: FactBatch -> [RawFact]
deserializeBatch batch = go (batchData batch)
  where
    go bytes
      | BS.null bytes = []
      | otherwise     =
          case readRawFact bytes of
            Nothing          -> []
            Just (fact, rest) -> fact : go rest

-- | Read one raw fact from a byte string.
-- Returns the fact and remaining bytes, or Nothing on parse failure.
readRawFact :: ByteString -> Maybe (RawFact, ByteString)
readRawFact bytes = do
  (pid,  r1) <- readWord64LE bytes
  (dlen, r2) <- readWord32LE r1
  let dlen' = fromIntegral dlen
  if BS.length r2 < dlen'
    then Nothing
    else Just (RawFact pid (BS.take dlen' r2), BS.drop dlen' r2)

-- ── Binary reading helpers ────────────────────────────────────────────────────

readWord64LE :: ByteString -> Maybe (Word64, ByteString)
readWord64LE bs
  | BS.length bs < 8 = Nothing
  | otherwise =
      let (w, rest) = BS.splitAt 8 bs
          val = foldr (\(i, b) acc -> acc + fromIntegral b * (256 :: Word64)^(i :: Int))
                      0
                      (zip [0..7] (BS.unpack w))
      in Just (val, rest)

readWord32LE :: ByteString -> Maybe (Word32, ByteString)
readWord32LE bs
  | BS.length bs < 4 = Nothing
  | otherwise =
      let (w, rest) = BS.splitAt 4 bs
          val = foldr (\(i, b) acc -> acc + fromIntegral b * (256 :: Word32)^(i :: Int))
                      0
                      (zip [0..3] (BS.unpack w))
      in Just (val, rest)

readTextLE :: ByteString -> Maybe (Text, ByteString)
readTextLE bs = do
  (len, rest) <- readWord32LE bs
  let len' = fromIntegral len
  if BS.length rest < len'
    then Nothing
    else Just ( Text.decodeUtf8 (BS.take len' rest)
              , BS.drop len' rest )

-- ── Fact interpretation ───────────────────────────────────────────────────────

-- | Try to interpret a raw fact as a DefinitionFact.
toDefinition :: RawFact -> Maybe DefinitionFact
toDefinition fact
  | rawPid fact /= pidDefinition = Nothing
  | otherwise = parseDefinition (rawData fact)

-- | Try to interpret a raw fact as a ReferenceFact.
toReference :: RawFact -> Maybe ReferenceFact
toReference fact
  | rawPid fact /= pidReference = Nothing
  | otherwise = parseReference (rawData fact)

-- | Try to interpret a raw fact as a ModuleFact.
toModule :: RawFact -> Maybe ModuleFact
toModule fact
  | rawPid fact /= pidModule = Nothing
  | otherwise = parseModule (rawData fact)

-- | Convert a raw fact to a QueryResult.
toQueryResult :: RawFact -> Maybe QueryResult
toQueryResult fact
  | rawPid fact == pidDefinition =
      DefinitionResult <$> parseDefinition (rawData fact)
  | rawPid fact == pidReference  =
      ReferenceResult  <$> parseReference  (rawData fact)
  | rawPid fact == pidModule     =
      ModuleResult     <$> parseModule     (rawData fact)
  | rawPid fact == pidImport     =
      ImportResult     <$> parseImport     (rawData fact)
  | otherwise = Nothing

-- ── Fact parsers ──────────────────────────────────────────────────────────────

-- | Parse a DefinitionFact from raw bytes.
-- Format matches serializeDef in HIE.hs:
--   word32(name_len) + name_bytes
--   word32(module_len) + module_bytes
--   span bytes
parseDefinition :: ByteString -> Maybe DefinitionFact
parseDefinition bs = do
  (name,   r1) <- readTextLE bs
  (modName, r2) <- readTextLE r1
  span_          <- parseSpan r2
  return DefinitionFact
    { defName   = name
    , defModule = modName
    , defSpan   = fst span_
    , defType   = Nothing
    }

-- | Parse a ReferenceFact from raw bytes.
parseReference :: ByteString -> Maybe ReferenceFact
parseReference bs = do
  (name,   r1) <- readTextLE bs
  (modName, r2) <- readTextLE r1
  (span_, r3)   <- parseSpan r2
  (target, _)   <- readTextLE r3
  return ReferenceFact
    { refName   = name
    , refModule = modName
    , refSpan   = span_
    , refTarget = if Text.null target then Nothing else Just target
    }

-- | Parse a ModuleFact from raw bytes.
parseModule :: ByteString -> Maybe ModuleFact
parseModule bs = do
  (name, r1) <- readTextLE bs
  (file, _)  <- readTextLE r1
  return ModuleFact
    { modName = name
    , modFile = SrcFile file
    }

-- | Parse an ImportFact from raw bytes.
parseImport :: ByteString -> Maybe ImportFact
parseImport bs = do
  (from,   r1) <- readTextLE bs
  (target, r2) <- readTextLE r1
  (qual,   r3) <- readQual   r2
  (alias,  _)  <- readTextLE r3
  return ImportFact
    { impFrom      = from
    , impTarget    = target
    , impQualified = qual
    , impAlias     = if Text.null alias then Nothing else Just alias
    }

-- | Parse a SrcSpan from bytes.
-- Format matches encodeSpan in HIE.hs.
parseSpan :: ByteString -> Maybe (SrcSpan, ByteString)
parseSpan bs = do
  (file, r1)       <- readTextLE bs
  (startLine, r2)  <- readWord32LE r1
  (startCol,  r3)  <- readWord32LE r2
  (endLine,   r4)  <- readWord32LE r3
  (endCol,    r5)  <- readWord32LE r4
  return ( SrcSpan
             { spanFile  = SrcFile file
             , spanStart = SrcPos (fromIntegral startLine)
                                  (fromIntegral startCol)
             , spanEnd   = SrcPos (fromIntegral endLine)
                                  (fromIntegral endCol)
             }
         , r5
         )

-- | Read a bool (Word8) as qualified flag.
readQual :: ByteString -> Maybe (Bool, ByteString)
readQual bs
  | BS.null bs = Nothing
  | otherwise  = Just (BS.head bs /= 0, BS.tail bs)

-- ── Filter helpers ────────────────────────────────────────────────────────────

matchesPid :: Word64 -> RawFact -> Bool
matchesPid pid fact = rawPid fact == pid

matchesName :: Text -> RawFact -> Bool
matchesName name fact =
  case toQueryResult fact of
    Just (DefinitionResult d) -> defName d == name
    Just (ReferenceResult  r) -> refName r == name
    _                         -> False

matchModule :: Text -> RawFact -> Maybe QueryResult
matchModule mn fact =
  case toQueryResult fact of
    Just r@(DefinitionResult d) | defModule d == mn -> Just r
    Just r@(ReferenceResult  r') | refModule r' == mn -> Just r
    Just r@(ModuleResult m)     | modName m == mn -> Just r
    _                                                    -> Nothing
