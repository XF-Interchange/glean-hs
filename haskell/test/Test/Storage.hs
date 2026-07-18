module Test.Storage (spec) where

import Test.Hspec
import System.IO.Temp (withSystemTempDirectory)
import qualified Data.ByteString as BS
import Glean.Storage
import Glean.RocksDB (RocksDB)

spec :: Spec
spec = describe "Glean.Storage" $ do

  it "can store and retrieve a fact batch" $ do
    withSystemTempDirectory "glean-test" $ \dir -> do
      let config = defaultDbConfig dir
      withStorage config $ \(db :: RocksDB) -> do
        let batch = FactBatch
              { batchData       = BS.pack [1,2,3,4,5]
              , batchFirstId    = 1024
              , batchCount      = 1
              , batchPredicates = mempty
              }
        store db batch
        result <- retrieve db
        result `shouldSatisfy` (/= Nothing)

  it "returns Nothing for empty database" $ do
    withSystemTempDirectory "glean-test" $ \dir -> do
      let config = defaultDbConfig dir
      withStorage config $ \(db :: RocksDB) -> do
        result <- retrieve db
        result `shouldBe` Nothing

  it "persists fact count across connections" $ do
    withSystemTempDirectory "glean-test" $ \dir -> do
      let config = defaultDbConfig dir
      let batch = FactBatch
            { batchData       = BS.pack [1,2,3]
            , batchFirstId    = 1024
            , batchCount      = 5
            , batchPredicates = mempty
            }
      -- Write in first connection
      withStorage config $ \(db :: RocksDB) -> do
        store db batch
        props <- properties db
        propFactCount props `shouldBe` 5
