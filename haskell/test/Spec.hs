module Main (main) where

import Test.Hspec
import qualified Test.FFI
import qualified Test.Storage
import qualified Test.Indexer

main :: IO ()
main = hspec $ do
  Test.FFI.spec
  Test.Storage.spec
  Test.Indexer.spec
