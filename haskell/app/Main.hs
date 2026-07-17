-- | glean-hs command line tool.
--
-- Index Haskell projects and query the resulting fact database.
--
-- Usage:
--   glean-hs index --hie-dir .hie --db /tmp/mydb
--   glean-hs query --db /tmp/mydb "validateCDTCode"
--   glean-hs query --db /tmp/mydb "ref:validateCDTCode"
--   glean-hs query --db /tmp/mydb "mod:Glean.Storage"

module Main (main) where

import Control.Exception (catch, SomeException, displayException)
import Data.Text (Text)
import qualified Data.Text as Text
import Options.Applicative
import System.Exit (exitFailure)
import System.IO (hPutStrLn, stderr)

import Glean.RocksDB (RocksDB)
import Glean.Storage
import Glean.Indexer.HIE
import Glean.Indexer.Types
import Glean.Query

-- ── CLI options ───────────────────────────────────────────────────────────────

data Command
  = Index IndexOptions
  | Query QueryOptions
  | Stats StatsOptions
  deriving (Show)

data IndexOptions = IndexOptions
  { idxHieDir   :: FilePath
  , idxDbPath   :: FilePath
  , idxVerbose  :: Bool
  , idxMaxFiles :: Maybe Int
  } deriving (Show)

data QueryOptions = QueryOptions
  { qryDbPath :: FilePath
  , qryQuery  :: Text
  } deriving (Show)

data StatsOptions = StatsOptions
  { stDbPath :: FilePath
  } deriving (Show)

-- ── Parsers ───────────────────────────────────────────────────────────────────

indexOptions :: Parser IndexOptions
indexOptions = IndexOptions
  <$> strOption
        ( long "hie-dir"
       <> metavar "DIR"
       <> value ".hie"
       <> showDefault
       <> help "Directory containing .hie files" )
  <*> strOption
        ( long "db"
       <> metavar "PATH"
       <> help "Path to the glean-hs database" )
  <*> switch
        ( long "verbose"
       <> short 'v'
       <> help "Print progress information" )
  <*> optional (option auto
        ( long "max-files"
       <> metavar "N"
       <> help "Maximum number of HIE files to index" ))

queryOptions :: Parser QueryOptions
queryOptions = QueryOptions
  <$> strOption
        ( long "db"
       <> metavar "PATH"
       <> help "Path to the glean-hs database" )
  <*> ( Text.pack <$> argument str
          ( metavar "QUERY"
         <> help "Query string. Prefix with ref: for references, mod: for modules" ))

statsOptions :: Parser StatsOptions
statsOptions = StatsOptions
  <$> strOption
        ( long "db"
       <> metavar "PATH"
       <> help "Path to the glean-hs database" )

commandParser :: Parser Command
commandParser = subparser
  ( command "index"
      ( info (Index <$> indexOptions)
             (progDesc "Index a Haskell project from HIE files") )
 <> command "query"
      ( info (Query <$> queryOptions)
             (progDesc "Query the fact database") )
 <> command "stats"
      ( info (Stats <$> statsOptions)
             (progDesc "Show database statistics") )
  )

opts :: ParserInfo Command
opts = info (commandParser <**> helper)
  ( fullDesc
 <> progDesc "glean-hs: Docker-free Haskell code indexing"
 <> header "glean-hs - XF-Interchange LLC" )

-- ── Command handlers ──────────────────────────────────────────────────────────

runIndex :: IndexOptions -> IO ()
runIndex options = do
  let config = defaultDbConfig (idxDbPath options)
  let idxCfg = defaultIndexConfig
        { cfgHieDir   = idxHieDir   options
        , cfgVerbose  = idxVerbose  options
        , cfgMaxFiles = idxMaxFiles options
        }
  withStorage config $ \(db :: RocksDB) -> do
    stats <- indexProject db idxCfg
    putStrLn $ "Indexed " ++ show (statsFilesIndexed stats) ++ " files"
    putStrLn $ "  " ++ show (statsDefsFound    stats) ++ " definitions"
    putStrLn $ "  " ++ show (statsRefsFound    stats) ++ " references"
    putStrLn $ "  " ++ show (statsModulesFound stats) ++ " modules"
    when (statsErrors stats > 0) $
      putStrLn $ "  " ++ show (statsErrors stats) ++ " errors"
  where
    when True  action = action
    when False _      = return ()

runQuery :: QueryOptions -> IO ()
runQuery options = do
  let config = (defaultDbConfig (qryDbPath options))
        { dbReadOnly = True
        , dbCreate   = False
        }
  withStorage config $ \(db :: RocksDB) -> do
    let q = qryQuery options
    putStrLn $ "Query: " ++ Text.unpack q
    if Text.pack "ref:" `Text.isPrefixOf` q
      then do
        refs <- findReferences db (Text.drop 4 q)
        if null refs
          then putStrLn "No references found."
          else do
            putStrLn $ "Found " ++ show (length refs) ++ " reference(s):"
            mapM_ printRef refs
      else if Text.pack "mod:" `Text.isPrefixOf` q
      then do
        facts <- findByModule db (Text.drop 4 q)
        if null facts
          then putStrLn "No facts found for module."
          else putStrLn $ "Found " ++ show (length facts) ++ " fact(s) in module."
      else do
        defs <- findDefinitions db q
        if null defs
          then putStrLn "No definitions found."
          else do
            putStrLn $ "Found " ++ show (length defs) ++ " definition(s):"
            mapM_ printDef defs

runStats :: StatsOptions -> IO ()
runStats options = do
  let config = (defaultDbConfig (stDbPath options))
        { dbReadOnly = True
        , dbCreate   = False
        }
  withStorage config $ \(db :: RocksDB) -> do
    props <- properties db
    stats <- predicateStats db
    putStrLn $ "Database: " ++ stDbPath options
    putStrLn $ "  Version:    " ++ show (propVersion     props)
    putStrLn $ "  First ID:   " ++ show (propFirstId     props)
    putStrLn $ "  Next ID:    " ++ show (propFirstFreeId props)
    putStrLn $ "  Facts:      " ++ show (propFactCount   props)
    putStrLn $ "  Predicates: " ++ show (length stats)

-- ── Display helpers ───────────────────────────────────────────────────────────

printDef :: DefinitionFact -> IO ()
printDef d = putStrLn $
  "  " ++ Text.unpack (defName d) ++
  " [" ++ Text.unpack (defModule d) ++ "]" ++
  " line " ++ show (posLine (spanStart (defSpan d)))

printRef :: ReferenceFact -> IO ()
printRef r = putStrLn $
  "  " ++ Text.unpack (refName r) ++
  " [" ++ Text.unpack (refModule r) ++ "]" ++
  " line " ++ show (posLine (spanStart (refSpan r))) ++
  maybe "" (\t -> " -> " ++ Text.unpack t) (refTarget r)

-- ── Main ──────────────────────────────────────────────────────────────────────

main :: IO ()
main = do
  cmd <- execParser opts
  result <- catch (run cmd >> return True)
    (\e -> do
      hPutStrLn stderr $ "Error: " ++ displayException (e :: SomeException)
      return False)
  if result
    then return ()
    else exitFailure

run :: Command -> IO ()
run (Index options) = runIndex options
run (Query options) = runQuery options
run (Stats options) = runStats options
