# testone

Шутер с видом сверху: бесконечные волны врагов, движение в духе CPMA (распрыжка, прыжки от стен, рокет-джампы), четыре оружия и враги-рэгдоллы.

## Что нужно

- **Видеокарта с аппаратной трассировкой лучей**: NVIDIA RTX 20xx и новее или AMD RX 6000 и новее, свежий драйвер с поддержкой Vulkan. Освещение в игре полностью трассируется (Bevy Solari), на картах без RT игра не запустится. Шумоподавитель DLSS Ray Reconstruction включается только на RTX. Без него картинка будет зернистой, но играть можно.
- **Linux x86_64** (игру разрабатывают и проверяют на нём). На Windows тоже может собраться, но это не проверялось.
- Около 10 ГБ места на диске для сборки и 10–30 минут на первую компиляцию.

## Установка

### 1. Rust

Поставьте [rustup](https://rustup.rs). Нужную nightly-версию Rust он скачает сам при первой сборке: она указана в `rust-toolchain.toml`.

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. Системные пакеты

Нужны clang (для сборки обвязки DLSS), заголовки Vulkan и библиотеки, которые нужны Bevy для звука, ввода и окон.

Arch / Manjaro:

```sh
sudo pacman -S --needed base-devel clang vulkan-headers vulkan-icd-loader alsa-lib systemd-libs libxkbcommon wayland libx11 libxcursor libxrandr libxi
```

Ubuntu / Debian:

```sh
sudo apt install build-essential pkg-config clang libclang-dev libvulkan-dev libasound2-dev libudev-dev libxkbcommon-x11-0 libwayland-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev
```

Fedora:

```sh
sudo dnf install gcc-c++ clang clang-devel vulkan-headers vulkan-loader-devel alsa-lib-devel systemd-devel libxkbcommon-x11-devel wayland-devel libX11-devel libXcursor-devel libXrandr-devel libXi-devel
```

### 3. NVIDIA DLSS SDK

Без DLSS SDK игра **не соберётся**, даже на картах AMD. По лицензии NVIDIA его нельзя хранить в этом репозитории, поэтому каждый скачивает его сам. Нужна строго версия **v310.9.1**:

```sh
git clone --branch v310.9.1 --depth 1 https://github.com/NVIDIA/DLSS.git ~/DLSS
```

Затем укажите пути к SDK в переменных окружения. Добавьте эти строки в `~/.bashrc` или `~/.zshrc` и откройте новый терминал:

```sh
export DLSS_SDK="$HOME/DLSS"   # путь обязательно абсолютный
export VULKAN_SDK=/usr
```

В `.cargo/config.toml` тоже прописан путь к SDK, но он относится к компьютеру автора. Переменные из вашего окружения важнее, поэтому этот файл менять не нужно.

### 4. Код

Попросите автора добавить вас в репозиторий и [добавьте свой SSH-ключ](https://docs.github.com/ru/authentication/connecting-to-github-with-ssh) в аккаунт. Затем:

```sh
git clone git@github.com:andreyflat/testone.git
cd testone
```

## Запуск

```sh
cargo run --release
```

Первая сборка идёт долго, следующие быстрее. Для разработки удобнее `cargo run`: игра соберётся быстрее, но будет работать медленнее.

Обновить игру до последней версии:

```sh
git pull
cargo run --release
```

## Управление

| Клавиша | Действие |
| --- | --- |
| W A S D | Движение |
| Мышь | Прицел |
| ЛКМ | Стрелять |
| Пробел | Прыжок (если держать, прыжок повторяется сразу после приземления; у стены получается прыжок от стены) |
| 1 / 2 / 3 / 4 | Кастет / пулемёт / рейлган / ракетница |
| R | Вернуть игрока в центр карты, если он застрял |
| Esc | Выйти |

## Если что-то не работает

- **`DLSS_SDK environment variable not set`** или ошибки про `nvsdk_ngx`: SDK не скачан, указан неверный путь или версия не v310.9.1. Проверьте `echo $DLSS_SDK` и что в этой папке есть `lib/Linux_x86_64`.
- **`Unable to find libclang`**: не установлен clang или libclang-dev (шаг 2).
- **Ошибки линковки `alsa`, `udev`, `xkbcommon`**: не хватает системных пакетов из шага 2.
- **Игра сразу закрывается, в логе ошибки про ray tracing или adapter**: видеокарта или драйвер не поддерживают трассировку лучей в Vulkan. Проверьте вывод `vulkaninfo | grep -i ray_tracing`.
- **`DLSS Ray Reconstruction недоступен на этой системе`**: это предупреждение, а не ошибка. Так и должно быть на картах не от NVIDIA.
- Ошибки валидации Vulkan, в которых упоминается DLSS, известны и безопасны.
