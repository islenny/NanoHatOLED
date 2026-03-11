# NanoHatOLED Rust (WSL -> DietPi)

| English | 中文 |
| --- | --- |
| This project rewrites FriendlyARM `NanoHatOLED` in Rust and reproduces the original C + Python demo behavior. | 這個專案使用 Rust 重構 FriendlyARM `NanoHatOLED`，重現原本 C + Python demo 的行為。 |

## Features

| English | 中文 |
| --- | --- |
| Show FriendlyELEC logo at boot (2 seconds) | 開機顯示 FriendlyELEC Logo（2 秒） |
| On Page 0, pressing K1 toggles between the logo and Page 0 | 在頁面 0 按下 K1 會在 Logo 與頁面 0 之間交替切換 |
| On Page 1, pressing K2 cycles between Page 1 and single-metric pages (IP / CPU Load / Memory / Disk / CPU Temp) | 在頁面 1 按下 K2 會在頁面 1 與單項數據頁（IP / CPU Load / Memory / Disk / CPU Temp）間循環切換 |
| On the power menu page, K1 cycles Cancel/Reboot/Shutdown and K2 executes the selected action | 在電源選單頁中，K1 於 Cancel/Reboot/Shutdown 間循環，K2 執行當前選項 |
| Three-button control (K1/K2/K3) | 三鍵控制（K1/K2/K3） |
| Page 0: date / disk / time (using vinewx/NanoHatOLED font and layout settings) | 頁面 0：日期 / 磁碟 / 時間 (套用 vinewx/NanoHatOLED 字型與畫面排版) |
| Page 1: IP / CPU load / memory / disk / CPU temp | 頁面 1：IP / CPU Load / 記憶體 / 磁碟 / CPU 溫度 |
| Power menu page: Cancel / Reboot / Shutdown | 電源選單頁：Cancel / Reboot / Shutdown |
| Auto turn off OLED after inactivity (wake on any button) | 長時間未按鍵自動關閉 OLED（按任一鍵喚醒） |
| Single-instance PID lock (prevent duplicate startup) | 單實例 PID 鎖檔（避免重複啟動） |

## Project Structure

| Path | English | 中文 |
| --- | --- | --- |
| `src/main.rs` | Main state machine and page flow | 主狀態機與頁面流程 |
| `src/display.rs` | SSD1306 I2C transfer and initialization | SSD1306 I2C 傳輸與初始化 |
| `src/framebuffer.rs` | 128x64 monochrome framebuffer and basic drawing | 128x64 單色 framebuffer 與基本繪圖 |
| `src/text.rs` | DejaVuSansMono font rendering | DejaVuSansMono 字型渲染 |
| `src/buttons.rs` | GPIO rising-edge listener (`/dev/gpiochip*`) | GPIO rising edge 監聽（`/dev/gpiochip*`） |
| `src/metrics.rs` | System metrics collection | 系統資訊收集 |
| `deploy/nanohat-oled-rs.service` | systemd service | systemd 服務 |
| `scripts/build-wsl.sh` | WSL cross-build script | WSL 交叉編譯腳本 |
| `scripts/install-on-dietpi.sh` | DietPi installation script | DietPi 安裝腳本 |

## GPIO/I2C Configuration

| Item | English | 中文 |
| --- | --- | --- |
| I2C bus | `/dev/i2c-0` | `/dev/i2c-0` |
| I2C address | `0x3c` | `0x3c` |
| GPIO chip | `/dev/gpiochip1` (NanoPi) | `/dev/gpiochip1`（NanoPi） |
| K1/K2/K3 line | `0 / 2 / 3` | `0 / 2 / 3` |
| Idle OLED power-off seconds | `30` (`0` to disable) | `30`（設 `0` 可停用） |

| English | 中文 |
| --- | --- |
| If board mapping differs, edit: `/etc/default/nanohat-oled-rs` | 若板子對應不同，可編輯：`/etc/default/nanohat-oled-rs` |
| Inspect GPIO lines: `gpioinfo /dev/gpiochip1` | 查詢 GPIO 線路：`gpioinfo /dev/gpiochip1` |
| On DietPi, verify I2C is enabled before running this service. | 在 DietPi 上執行本服務前，請先確認 I2C 已啟用。 |

```bash
# Check whether i2c0 overlay is enabled in /boot/dietpiEnv.txt
grep -nE '^(overlays|dtoverlay)=' /boot/dietpiEnv.txt
if grep -qE '\bi2c0\b' /boot/dietpiEnv.txt; then
  echo "i2c0 overlay: enabled"
else
  echo "i2c0 overlay: NOT found"
  echo "Adding i2c0 to /boot/dietpiEnv.txt ..."
  sudo cp /boot/dietpiEnv.txt /boot/dietpiEnv.txt.bak

  if grep -q '^overlays=' /boot/dietpiEnv.txt; then
    sudo sed -i -E '/^overlays=/ { /i2c0/! s/$/ i2c0/; }' /boot/dietpiEnv.txt
  else
    echo 'overlays=i2c0' | sudo tee -a /boot/dietpiEnv.txt >/dev/null
  fi

  echo "i2c0 added. Reboot to apply:"
  echo "  sudo reboot"
fi

# Verify I2C device nodes
ls -l /dev/i2c-*
i2cdetect -l

# Check OLED address (default 0x3c) on bus 0
sudo i2cdetect -y 0
```

## Build In WSL

| Step | English | 中文 |
| --- | --- | --- |
| 1 | Install Rust and toolchains | 安裝 Rust 與工具鏈 |
| 2 | Build for your target architecture | 依目標架構編譯 |

```bash
sudo apt-get update
sudo apt-get install -y curl build-essential

# Optional: only needed when building GNU targets
sudo apt-get install -y gcc-aarch64-linux-gnu gcc-arm-linux-gnueabihf

curl https://sh.rustup.rs -sSf | sh
source "$HOME/.cargo/env"

# Recommended for 64-bit DietPi (static binary, avoids glibc mismatch)
./scripts/build-wsl.sh aarch64-unknown-linux-musl

# 64-bit DietPi (GNU/glibc dynamic binary)
./scripts/build-wsl.sh aarch64-unknown-linux-gnu

# 32-bit DietPi
./scripts/build-wsl.sh armv7-unknown-linux-gnueabihf
```

| English | 中文 |
| --- | --- |
| Output path: `target/<target>/release/nanohat-oled-rs` | 輸出路徑：`target/<target>/release/nanohat-oled-rs` |

## Deploy To DietPi

| Step | English | 中文 |
| --- | --- | --- |
| 1 | Copy files to the board | 拷貝檔案到板子 |
| 2 | Install on DietPi | 在 DietPi 安裝 |
| 3 | Verify service status and logs | 確認服務狀態與日誌 |

```bash
# 1) Copy files
scp target/aarch64-unknown-linux-musl/release/nanohat-oled-rs \
  dietpi@<BOARD_IP>:/tmp/nanohat-oled-rs
scp -r deploy scripts dietpi@<BOARD_IP>:/tmp/nanohat-oled-rs-files

# 2) Install
ssh dietpi@<BOARD_IP>
cd /tmp/nanohat-oled-rs-files
sudo ./scripts/install-on-dietpi.sh /tmp/nanohat-oled-rs

# 3) Verify
sudo systemctl status nanohat-oled-rs.service
sudo journalctl -u nanohat-oled-rs.service -f
```

## Troubleshooting

| English | 中文 |
| --- | --- |
| If you see `/lib/aarch64-linux-gnu/libc.so.6: version 'GLIBC_2.32' not found`, your binary was built against a newer glibc than the DietPi system. | 若出現 `/lib/aarch64-linux-gnu/libc.so.6: version 'GLIBC_2.32' not found`，表示你的執行檔連結到比 DietPi 系統更新的 glibc。 |
| Rebuild with `aarch64-unknown-linux-musl` (static) and redeploy. | 請改用 `aarch64-unknown-linux-musl`（靜態連結）重新編譯後重新部署。 |

```bash
# Rebuild (WSL)
./scripts/build-wsl.sh aarch64-unknown-linux-musl

# Copy to DietPi
scp target/aarch64-unknown-linux-musl/release/nanohat-oled-rs \
  dietpi@<BOARD_IP>:/tmp/nanohat-oled-rs

# On DietPi
sudo /tmp/nanohat-oled-rs --help
```

## Local Debug (Without Hardware)

| English | 中文 |
| --- | --- |
| Run locally without hardware and skip actual poweroff | 本機不接硬體執行，並跳過真實關機 |

```bash
cargo run -- --disable-gpio --dry-run-poweroff
```

## Safety Note

| English | 中文 |
| --- | --- |
| In the power menu page, pressing K2 on `Reboot` or `Shutdown` will execute system reboot/shutdown. Use `--dry-run-poweroff` first during testing. | 在電源選單頁中，若選到 `Reboot` 或 `Shutdown` 再按 K2，會執行系統重啟/關機。測試時建議先加 `--dry-run-poweroff`。 |
