module Test.Indexer (spec) where

import Test.Hspec
import System.IO.Temp (withSystemTempDirectory)
import System.FilePath ((</>))
import Glean.Indexer.HIE
import Glean.Indexer.Types
import Glean.Storage
import Glean.RocksDB (RocksDB)

spec :: Spec
spec = describe "Glean.Indexer.HIE" $ do

  it "returns empty result for missing hie directory" $ do
    let config = defaultIndexConfig { cfgHieDir = "/nonexistent/.hie" }
    result <- indexHieDirectory config
    statsFilesIndexed (resultStats result) `shouldBe` 0
    statsErrors (resultStats result) `shouldBe` 0

  it "indexProject handles empty hie directory gracefully" $ do
    withSystemTempDirectory "glean-test" $ \dir -> do
      let dbConfig  = defaultDbConfig dir
      let idxConfig = defaultIndexConfig
            { cfgHieDir = dir </> ".hie" }
      withStorage dbConfig $ \(db :: RocksDB) -> do
        stats <- indexProject db idxConfig
        statsFilesIndexed stats `shouldBe` 0
        statsErrors stats `shouldBe` 0
