-- | Types for the glean-hs HIE indexer.
--
-- These types represent the facts we extract from GHC HIE files
-- and store in the Glean database.

module Glean.Indexer.Types
  ( -- * Source locations
    SrcFile (..)
  , SrcSpan (..)
  , SrcPos (..)

    -- * Code facts
  , DefinitionFact (..)
  , ReferenceFact (..)
  , ModuleFact (..)
  , ImportFact (..)

    -- * Indexed module
  , IndexedModule (..)
  , emptyIndexedModule

    -- * Predicate IDs (must match Angle schema)
  , pidDefinition
  , pidReference
  , pidModule
  , pidImport
  ) where

import Data.Text (Text)
import Data.Word (Word64)

-- ── Source locations ──────────────────────────────────────────────────────────

-- | A source file path.
newtype SrcFile = SrcFile { srcFilePath :: Text }
  deriving (Show, Eq, Ord)

-- | A position in a source file (1-indexed).
data SrcPos = SrcPos
  { posLine :: !Int
  , posCol  :: !Int
  } deriving (Show, Eq, Ord)

-- | A span in a source file.
data SrcSpan = SrcSpan
  { spanFile  :: !SrcFile
  , spanStart :: !SrcPos
  , spanEnd   :: !SrcPos
  } deriving (Show, Eq, Ord)

-- ── Code facts ────────────────────────────────────────────────────────────────

-- | A definition fact: a name defined at a location.
data DefinitionFact = DefinitionFact
  { defName   :: !Text       -- ^ The defined name (e.g. "validateCDTCode")
  , defModule :: !Text       -- ^ The module it belongs to
  , defSpan   :: !SrcSpan   -- ^ Where it's defined
  , defType   :: !(Maybe Text) -- ^ Type signature if available
  } deriving (Show, Eq)

-- | A reference fact: a name used at a location.
data ReferenceFact = ReferenceFact
  { refName   :: !Text       -- ^ The referenced name
  , refModule :: !Text       -- ^ The module containing the reference
  , refSpan   :: !SrcSpan   -- ^ Where it's referenced
  , refTarget :: !(Maybe Text) -- ^ The module where it's defined
  } deriving (Show, Eq)

-- | A module fact: a Haskell module.
data ModuleFact = ModuleFact
  { modName :: !Text         -- ^ Module name (e.g. "SeidoClaims.Validation")
  , modFile :: !SrcFile      -- ^ Source file
  } deriving (Show, Eq)

-- | An import fact: a module importing another.
data ImportFact = ImportFact
  { impFrom      :: !Text    -- ^ Importing module
  , impTarget    :: !Text    -- ^ Imported module
  , impQualified :: !Bool    -- ^ Is it a qualified import?
  , impAlias     :: !(Maybe Text) -- ^ Import alias if any
  } deriving (Show, Eq)

-- ── Indexed module ────────────────────────────────────────────────────────────

-- | All facts extracted from a single HIE file.
data IndexedModule = IndexedModule
  { idxModule      :: !ModuleFact
  , idxDefinitions :: ![DefinitionFact]
  , idxReferences  :: ![ReferenceFact]
  , idxImports     :: ![ImportFact]
  } deriving (Show, Eq)

-- | An empty indexed module.
emptyIndexedModule :: ModuleFact -> IndexedModule
emptyIndexedModule m = IndexedModule
  { idxModule      = m
  , idxDefinitions = []
  , idxReferences  = []
  , idxImports     = []
  }

-- ── Predicate IDs ─────────────────────────────────────────────────────────────
-- These must match the Angle schema definitions.
-- Using sequential IDs starting from Fid::LOWEST (1024).

-- | Predicate ID for definition facts.
pidDefinition :: Word64
pidDefinition = 1

-- | Predicate ID for reference facts.
pidReference :: Word64
pidReference = 2

-- | Predicate ID for module facts.
pidModule :: Word64
pidModule = 3

-- | Predicate ID for import facts.
pidImport :: Word64
pidImport = 4
