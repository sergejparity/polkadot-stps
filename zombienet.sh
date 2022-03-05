#!/bin/bash

POLKADOT_V=v0.9.17-rc4

print_help() {
  echo "🧟 Zombienet Ecosystem Performance Optimizations 🦾"
  echo ""
  echo "we are about to spin a polkadot relay chain with a parachain node with extrinsics to be tested against."
  echo "first, create a pallet for your team, including the extrinsics you want to run tests for."
  echo "make sure you read zombienet specs from it's official repo: https://github.com/paritytech/zombienet"
  echo "write the zombienet test specifications under the tests directory"
  echo "then, call this script:"
  echo "$ ./zombienet.sh init"
  echo "$ ./zombienet.sh test tests/examples/0001-simple-network.feature"
  echo "$ ./zombienet.sh spawn tests/examples/0001-simple-network.toml"
}

fetch_zombienet() {
  if [ ! -d zombienet ]; then
    echo "cloning zombienet..."
    git clone https://github.com/paritytech/zombienet.git -b feat-cumulus-based
    pushd zombienet
    echo "building zombienet..."
    npm install
    npm run build
    popd
  fi
}

fetch_polkadot() {
  if [ ! -s polkadot ]; then
    echo "fetching polkadot executable..."
    wget --quiet https://github.com/paritytech/polkadot/releases/download/$POLKADOT_V/polkadot
    chmod +x polkadot
  fi
}

build_collator() {
  if [ ! -s target/release/parachain-collator ]; then
    echo "building collator executable..."
    cargo build --release --quiet
  fi
}

zombienet_test() {
  zombienet_init
  node zombienet/dist/cli.js test --provider native $1
}

zombienet_spawn() {
  zombienet_init
  node zombienet/dist/cli.js spawn --provider native $1
}

zombienet_init() {
  if [ ! -d bin ]; then
     mkdir bin
  fi
  fetch_zombienet
  fetch_polkadot
  build_collator
}

subcommand=$1
case $subcommand in
  "" | "-h" | "--help")
    print_help
    ;;
  *)
    shift
    zombienet_${subcommand} $@
    if [ $? = 127 ]; then
      echo "Error: '$subcommand' is not a known subcommand." >&2
      echo "Run './zombienet.sh --help' for a list of known subcommands." >&2
        exit 1
    fi
  ;;
esac