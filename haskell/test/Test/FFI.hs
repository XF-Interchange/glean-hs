module Test.FFI (spec) where

import Test.Hspec
import System.IO.Temp (withSystemTempDirectory)
import Glean.FFI
import Glean.Storage
import Glean.RocksDB (RocksDB)

spec :: Spec
spec = describe "Glean.FFI" $ do

  it "can allocate and free a cache" $ do
    cache <- newCache (8 * 1024 * 1024)
    cap <- cacheCapacity cache
    cap `shouldBe` (8 * 1024 * 1024)
    freeCache cache

  it "can open and close a database" $ do
    withSystemTempDirectory "glean-test" $ \dir -> do
      let config = defaultDbConfig dir
      withStorage config $ \(db :: RocksDB) -> do
        props <- properties db
        propVersion props `shouldBe` 1
        propFirstId props `shouldBe` 1024
