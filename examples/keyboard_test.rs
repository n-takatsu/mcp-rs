use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::time::Duration;

/// 簡単なキーボード入力テスト
///
/// このプログラムは以下をテストします：
/// 1. Crossterm でのキーボード入力検出
/// 2. 'q' キーでの終了処理
/// 3. その他のキー入力の認識
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Keyboard Input Test for Dashboard");
    println!("Press any key to see input detection:");
    println!("  - 'q': Exit immediately");
    println!("  - 'h': Show help message");
    println!("  - Any other key: Display key info");
    println!("  - Ctrl+C: Force exit");
    println!();

    // Raw mode を有効化
    enable_raw_mode()?;

    let mut loop_count = 0;

    loop {
        loop_count += 1;

        // イベントをポーリング（100ms タイムアウト）
        if event::poll(Duration::from_millis(100))? {
            let event = event::read()?;

            match event {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('q') => {
                        println!("\r\n✅ 'q' pressed - Exiting gracefully...");
                        break;
                    }
                    KeyCode::Char('h') => {
                        println!("\r\n💡 Help: This is a keyboard input test");
                    }
                    KeyCode::Char(c) => {
                        println!("\r\n📝 Character pressed: '{}'", c);
                    }
                    KeyCode::Enter => {
                        println!("\r\n⏎ Enter key pressed");
                    }
                    KeyCode::Esc => {
                        println!("\r\n🔄 Escape key pressed");
                    }
                    _ => {
                        println!("\r\n🔧 Special key pressed: {:?}", key.code);
                    }
                },
                Event::Key(key) => {
                    // キーリリースイベントなど
                    println!("\r\n🔕 Key event (not press): {:?}", key);
                }
                _ => {
                    // マウスイベントなど
                    println!("\r\n🖱️ Other event: {:?}", event);
                }
            }
        } else {
            // タイムアウト（ノンブロッキングループ）
            if loop_count % 10 == 0 {
                print!("\r⏱️ Waiting for input... (loop {})", loop_count / 10);
                std::io::Write::flush(&mut std::io::stdout())?;
            }
        }

        // 短い待機でCPU使用率を抑制
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Raw mode を無効化
    disable_raw_mode()?;

    println!("\n🏁 Keyboard input test completed successfully!");
    println!("✅ Event loop exited cleanly via 'q' key press");

    Ok(())
}
