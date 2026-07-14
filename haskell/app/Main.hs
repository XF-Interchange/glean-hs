-- | glean-hs command line tool.
--
-- Index Haskell projects and query the resulting fact database.
--
-- Usage:
--   glean-hs index --hie-dir .hie --db /tmp/mydb
--   glean-hs query --db /tmp/mydb "where is validateCDTCode defined?"

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

-- ── CLI options ───────────────────────────────────────────────────────────────

data Command
  = Index IndexOptions
  | Query QueryOptions
  | Stats StatsOptions
  deriving (Show)

data IndexOptions = IndexOptions
  { idxHieDir  :: FilePath
  , idxDbPath  :: FilePath
  , idxVerbose :: Bool
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
         <> help "Query string" ))

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
    putStrLn $ "  " ++ show (statsDefsFound stats) ++ " definitions"
    putStrLn $ "  " ++ show (statsRefsFound stats) ++ " references"
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
    -- Query implementation — Phase 13 (Angle query language)
    putStrLn $ "Query: " ++ Text.unpack (qryQuery options)
    putStrLn "Query execution not yet implemented."
    putStrLn "Angle query language integration — coming in Phase 13."

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
