//! WebSocket Compression Module
//!
//! WebSocketメッセージの圧縮・解凍機能を提供

use crate::error::{Error, Result};
use crate::transport::websocket::transfer::CompressionType;
use flate2::read::{DeflateDecoder, GzDecoder};
use flate2::write::{DeflateEncoder, GzEncoder};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

/// 圧縮設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// 圧縮タイプ
    pub compression_type: CompressionType,
    /// 圧縮レベル (1-9, 低いほど高速、高いほど高圧縮)
    pub level: u32,
    /// 最小圧縮サイズ（このサイズ以下のメッセージは圧縮しない）
    pub min_size: usize,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            compression_type: CompressionType::Gzip,
            level: 6,       // デフォルト圧縮レベル
            min_size: 1024, // 1KB未満は圧縮しない
        }
    }
}

/// 圧縮マネージャー
#[derive(Debug, Clone)]
pub struct CompressionManager {
    config: CompressionConfig,
}

impl CompressionManager {
    /// 新しい圧縮マネージャーを作成
    pub fn new(config: CompressionConfig) -> Self {
        Self { config }
    }

    /// データを圧縮
    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        // サイズが小さい場合は圧縮しない
        if data.len() < self.config.min_size {
            return Ok(data.to_vec());
        }

        match self.config.compression_type {
            CompressionType::None => Ok(data.to_vec()),
            CompressionType::Gzip => self.compress_gzip(data),
            CompressionType::Zstd => self.compress_zstd(data),
        }
    }

    /// データを解凍
    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        match self.config.compression_type {
            CompressionType::None => Ok(data.to_vec()),
            CompressionType::Gzip => self.decompress_gzip(data),
            CompressionType::Zstd => self.decompress_zstd(data),
        }
    }

    /// Gzip圧縮
    fn compress_gzip(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut encoder = GzEncoder::new(Vec::new(), self.get_compression_level());
        encoder
            .write_all(data)
            .map_err(|e| Error::Compression(format!("Gzip compression failed: {}", e)))?;
        encoder
            .finish()
            .map_err(|e| Error::Compression(format!("Gzip finalization failed: {}", e)))
    }

    /// Gzip解凍
    fn decompress_gzip(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut decoder = GzDecoder::new(data);
        let mut decompressed = Vec::new();
        decoder
            .read_to_end(&mut decompressed)
            .map_err(|e| Error::Compression(format!("Gzip decompression failed: {}", e)))?;
        Ok(decompressed)
    }

    /// Deflate圧縮（Gzipヘッダーなし）
    pub fn compress_deflate(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < self.config.min_size {
            return Ok(data.to_vec());
        }

        let mut encoder = DeflateEncoder::new(Vec::new(), self.get_compression_level());
        encoder
            .write_all(data)
            .map_err(|e| Error::Compression(format!("Deflate compression failed: {}", e)))?;
        encoder
            .finish()
            .map_err(|e| Error::Compression(format!("Deflate finalization failed: {}", e)))
    }

    /// Deflate解凍
    pub fn decompress_deflate(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut decoder = DeflateDecoder::new(data);
        let mut decompressed = Vec::new();
        decoder
            .read_to_end(&mut decompressed)
            .map_err(|e| Error::Compression(format!("Deflate decompression failed: {}", e)))?;
        Ok(decompressed)
    }

    /// Zstd圧縮
    fn compress_zstd(&self, data: &[u8]) -> Result<Vec<u8>> {
        zstd::encode_all(data, self.config.level as i32)
            .map_err(|e| Error::Compression(format!("Zstd compression failed: {}", e)))
    }

    /// Zstd解凍
    fn decompress_zstd(&self, data: &[u8]) -> Result<Vec<u8>> {
        zstd::decode_all(data)
            .map_err(|e| Error::Compression(format!("Zstd decompression failed: {}", e)))
    }

    /// Brotli圧縮
    pub fn compress_brotli(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < self.config.min_size {
            return Ok(data.to_vec());
        }

        let mut compressed = Vec::new();
        let quality = self.config.level.min(11); // Brotliは0-11
        let lg_window_size = 22; // デフォルトウィンドウサイズ

        brotli::BrotliCompress(
            &mut std::io::Cursor::new(data),
            &mut compressed,
            &brotli::enc::BrotliEncoderParams {
                quality: quality as i32,
                lgwin: lg_window_size,
                ..Default::default()
            },
        )
        .map_err(|e| Error::Compression(format!("Brotli compression failed: {}", e)))?;

        Ok(compressed)
    }

    /// Brotli解凍
    pub fn decompress_brotli(&self, data: &[u8]) -> Result<Vec<u8>> {
        let mut decompressed = Vec::new();
        brotli::BrotliDecompress(&mut std::io::Cursor::new(data), &mut decompressed)
            .map_err(|e| Error::Compression(format!("Brotli decompression failed: {}", e)))?;
        Ok(decompressed)
    }

    /// 圧縮レベルを flate2::Compression に変換
    fn get_compression_level(&self) -> flate2::Compression {
        flate2::Compression::new(self.config.level.clamp(0, 9))
    }

    /// 圧縮率を計算（パーセンテージ）
    pub fn compression_ratio(&self, original_size: usize, compressed_size: usize) -> f64 {
        if original_size == 0 {
            return 0.0;
        }
        (1.0 - (compressed_size as f64 / original_size as f64)) * 100.0
    }

    /// メッセージを圧縮すべきか判定
    pub fn should_compress(&self, data: &[u8]) -> bool {
        data.len() >= self.config.min_size && self.config.compression_type != CompressionType::None
    }
}

/// 圧縮統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionStats {
    /// 圧縮前の総バイト数
    pub original_bytes: u64,
    /// 圧縮後の総バイト数
    pub compressed_bytes: u64,
    /// 圧縮率（パーセンテージ）
    pub compression_ratio: f64,
    /// 圧縮回数
    pub compression_count: u64,
    /// 解凍回数
    pub decompression_count: u64,
}

impl CompressionStats {
    /// 新しい統計を作成
    pub fn new() -> Self {
        Self {
            original_bytes: 0,
            compressed_bytes: 0,
            compression_ratio: 0.0,
            compression_count: 0,
            decompression_count: 0,
        }
    }

    /// 圧縮を記録
    pub fn record_compression(&mut self, original_size: usize, compressed_size: usize) {
        self.original_bytes += original_size as u64;
        self.compressed_bytes += compressed_size as u64;
        self.compression_count += 1;
        self.update_ratio();
    }

    /// 解凍を記録
    pub fn record_decompression(&mut self) {
        self.decompression_count += 1;
    }

    /// 圧縮率を更新
    fn update_ratio(&mut self) {
        if self.original_bytes > 0 {
            self.compression_ratio =
                (1.0 - (self.compressed_bytes as f64 / self.original_bytes as f64)) * 100.0;
        }
    }

    /// 節約されたバイト数
    pub fn bytes_saved(&self) -> u64 {
        self.original_bytes.saturating_sub(self.compressed_bytes)
    }
}

impl Default for CompressionStats {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gzip_compression() {
        let config = CompressionConfig {
            compression_type: CompressionType::Gzip,
            level: 6,
            min_size: 10,
        };
        let manager = CompressionManager::new(config);

        // より大きなデータを使用して圧縮効果を確認
        let data = b"Hello, World! This is a test message for compression. \
                      We need a longer message to ensure that the compressed \
                      size is actually smaller than the original. Let's add \
                      more repetitive text to make it compressible. Hello again! \
                      Hello once more! This should compress well.";
        let compressed = manager.compress(data).unwrap();
        let decompressed = manager.decompress(&compressed).unwrap();

        assert_eq!(data.to_vec(), decompressed);
        // 十分に大きなデータなら圧縮される
        assert!(compressed.len() < data.len());
    }

    #[test]
    fn test_deflate_compression() {
        let config = CompressionConfig {
            compression_type: CompressionType::Gzip,
            level: 6,
            min_size: 10,
        };
        let manager = CompressionManager::new(config);

        let data = b"Deflate compression test data. This should be compressed efficiently.";
        let compressed = manager.compress_deflate(data).unwrap();
        let decompressed = manager.decompress_deflate(&compressed).unwrap();

        assert_eq!(data.to_vec(), decompressed);
        assert!(compressed.len() < data.len());
    }

    #[test]
    fn test_zstd_compression() {
        let config = CompressionConfig {
            compression_type: CompressionType::Zstd,
            level: 3,
            min_size: 10,
        };
        let manager = CompressionManager::new(config);

        let data = b"Zstd is a fast compression algorithm with great compression ratios!";
        let compressed = manager.compress(data).unwrap();
        let decompressed = manager.decompress(&compressed).unwrap();

        assert_eq!(data.to_vec(), decompressed);
    }

    #[test]
    fn test_brotli_compression() {
        let config = CompressionConfig {
            compression_type: CompressionType::Gzip,
            level: 6,
            min_size: 10,
        };
        let manager = CompressionManager::new(config);

        let data = b"Brotli compression test! It should compress this text effectively.";
        let compressed = manager.compress_brotli(data).unwrap();
        let decompressed = manager.decompress_brotli(&compressed).unwrap();

        assert_eq!(data.to_vec(), decompressed);
        assert!(compressed.len() < data.len());
    }

    #[test]
    fn test_min_size_threshold() {
        let config = CompressionConfig {
            compression_type: CompressionType::Gzip,
            level: 6,
            min_size: 1024,
        };
        let manager = CompressionManager::new(config);

        // 小さいデータは圧縮されない
        let small_data = b"Small";
        let result = manager.compress(small_data).unwrap();
        assert_eq!(small_data.to_vec(), result);

        // 大きいデータは圧縮される
        let large_data = vec![b'A'; 2048];
        let compressed = manager.compress(&large_data).unwrap();
        assert!(compressed.len() < large_data.len());
    }

    #[test]
    fn test_should_compress() {
        let config = CompressionConfig {
            compression_type: CompressionType::Gzip,
            level: 6,
            min_size: 100,
        };
        let manager = CompressionManager::new(config);

        assert!(!manager.should_compress(b"Short"));
        let large_data = [b'X'; 200];
        assert!(manager.should_compress(&large_data));

        let no_compression_config = CompressionConfig {
            compression_type: CompressionType::None,
            level: 6,
            min_size: 100,
        };
        let no_compression_manager = CompressionManager::new(no_compression_config);
        let large_data = [b'X'; 200];
        assert!(!no_compression_manager.should_compress(&large_data));
    }

    #[test]
    fn test_compression_ratio() {
        let config = CompressionConfig::default();
        let manager = CompressionManager::new(config);

        let ratio = manager.compression_ratio(1000, 500);
        assert!((ratio - 50.0).abs() < 0.01);

        let ratio2 = manager.compression_ratio(1000, 250);
        assert!((ratio2 - 75.0).abs() < 0.01);
    }

    #[test]
    fn test_compression_stats() {
        let mut stats = CompressionStats::new();

        stats.record_compression(1000, 400);
        assert_eq!(stats.original_bytes, 1000);
        assert_eq!(stats.compressed_bytes, 400);
        assert_eq!(stats.compression_count, 1);
        assert!((stats.compression_ratio - 60.0).abs() < 0.01);

        stats.record_compression(2000, 800);
        assert_eq!(stats.original_bytes, 3000);
        assert_eq!(stats.compressed_bytes, 1200);
        assert_eq!(stats.compression_count, 2);
        assert!((stats.compression_ratio - 60.0).abs() < 0.01);

        assert_eq!(stats.bytes_saved(), 1800);
    }

    #[test]
    fn test_different_compression_levels() {
        let levels = vec![1, 3, 6, 9];
        let data = vec![b'A'; 10000];

        for level in levels {
            let config = CompressionConfig {
                compression_type: CompressionType::Gzip,
                level,
                min_size: 10,
            };
            let manager = CompressionManager::new(config);

            let compressed = manager.compress(&data).unwrap();
            let decompressed = manager.decompress(&compressed).unwrap();

            assert_eq!(data, decompressed);
        }
    }
}
