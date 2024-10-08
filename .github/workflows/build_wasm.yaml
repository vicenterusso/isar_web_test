name: WASM Build and Release

on:
  push:
    tags:
      - "wasm-*"

jobs:
  build_wasm:
    name: Build WASM
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v3
      
      - name: Prepare Build
        uses: ./.github/actions/prepare-build
      
      - name: Set Isar Version
        run: echo "ISAR_VERSION=${{ github.ref_name }}" >> $GITHUB_ENV
      
      - name: Build WASM
        run: bash tool/build_wasm.sh
      
      - name: Upload WASM to Release
        uses: svenstaro/upload-release-action@v2
        with:
          repo_token: ${{ secrets.GITHUB_TOKEN }}
          file: isar.wasm
          asset_name: isar.wasm
          tag: ${{ github.ref }}