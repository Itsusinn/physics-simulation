# Three Body · 3-4-5 三体实验室

浏览器中的经典毕达哥拉斯三体模拟。使用经确认的 **Rust / WASM + Bevy 0.20.0 + bevy_egui 0.43.1 + wgpu + Trunk**。用户已确认将物理计算迁至 CPU，使用 **Rust `ode_solvers` 0.6.2 的 DOP853 + f64**；WebGPU 负责天体、轨迹与网格渲染。积分状态、初始条件和守恒量诊断全部保留双精度，只在上传显示数据时转换为 f32。没有 JavaScript 物理引擎或额外前端框架。

## 运行

需要 Rust 1.97.1+、`wasm32-unknown-unknown` 目标，以及支持 WebGPU 的浏览器。使用 localhost 或 HTTPS，不能直接打开 HTML 文件。

```powershell
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.21.14 --locked
trunk serve --release
```

打开 <http://127.0.0.1:8080>。项目内已下载校验过的 Windows Trunk，当前工作区也可直接执行：

```powershell
.\dev.ps1 -Release
```

脚本优先使用 `.tools/trunk.exe`，否则使用 PATH 中的 Trunk；同时兼容宿主环境的 `NO_COLOR=1`。`.tools/` 不属于源代码，换机器时按上述命令安装 Trunk。

构建可部署静态文件：

```powershell
.\dev.ps1 -Action build -Release
# 或 trunk build --release
```

输出位于 `dist/`。Trunk 负责安装与 Cargo.lock 匹配的 wasm-bindgen 工具，以及 `Trunk.toml` 固定的 Binaryen 133。用户已确认引入 wasm-opt；release 构建使用 `-Oz`，清理符号名称和 producers 信息。开发构建不执行 wasm-opt。

Bevy 关闭默认特性，仅启用窗口、输入、资源和渲染所需组件；WASM 只启用 WebGPU，不启用 WebGL、3D PBR、音频或 Bevy UI。参数界面由 bevy_egui 绘制，保留 egui 默认字体。Bevy 管理窗口、GPU 和渲染调度，定制 WGSL 通过 bevy_egui 绘制回调执行。原生目标支持 `cargo run`，也用于渲染集成测试。物理与调度测试本身不需要 GPU。

release 使用 `opt-level = "s"`、完整 LTO 和单个 codegen unit；迁移前实测 `s` 比 `z` 更小；Bevy 产物需单独测量。不要在 Cargo 中提前设置 `strip = "symbols"`：它会移除 wasm-opt 自动识别指令集需要的 `target_features` 元数据。符号清理由最后的 wasm-opt 阶段完成。优化构建耗时更长，生产产物的函数名称被删除，调试时使用开发构建。

2026-10-15 迁移 DOP853 之前，在 Windows、Rust 1.98.1、Trunk 0.21.14 下的 release 体积优化实测（字节；gzip-9 使用相同参数估算，以下为历史基线）：

| 配置 | WASM 原始大小 | gzip-9 |
| --- | ---: | ---: |
| 原始配置：WebGL 特性、opt-level 2、thin LTO、无 wasm-opt | 10,458,286 | 3,297,478 |
| 仅移除 WASM 的 WebGL 依赖 | 6,187,380 | 2,134,054 |
| 最终配置：WebGPU-only、opt-level s、fat LTO、wasm-opt | 3,963,923 | 1,774,508 |

最终原始大小减少 62.1%，gzip-9 估算减少 46.2%。不同平台、工具链和后续代码修改可能改变体积。

2026-10-15 迁移 Bevy 0.20.0 后，在相同 Windows 工具链和 `s` / fat LTO / wasm-opt `-Oz` 下，WASM 原始大小为 **15,038,672 字节**，gzip-9 估算为 **5,016,041 字节**。这是引擎窗口、ECS、资源和渲染基础设施带来的新体积基线，不能沿用 eframe 时期的体积结论。用户已确认将 CI 原始体积预算调整为 18,000,000 字节。

## 持续集成

源码仓库：<https://github.com/Itsusinn/three-body>。

推送到 `main` 或创建 pull request 后，GitHub Actions 使用 `rust-toolchain.toml` 固定的 Rust 1.98.1 和经过 SHA-256 校验的 Trunk 0.21.14，执行格式检查、WASM Clippy 检查和锁定依赖的 release 构建。生成的 `three-body-web` artifact 包含完整 HTML / JS / WASM 和记录源码提交的 `version.json`，保留 14 天。工作流也支持手动运行和由网站工作流调用。

CI 同时校验 Binaryen 133 的下载摘要，并在 Actions summary 与产物 `size-report.json` 中记录 WASM 原始字节数和 gzip-9 估算。WASM 总体积上限为 18,000,000 字节；超过时构建失败。gzip 数值用于对比，不代表服务器实际传输大小，也不会生成用于部署的 `.gz` 文件。

CI 目前执行上述 WASM 检查和构建；原生物理与渲染测试在本地执行，其中仅渲染测试需要 GPU adapter。应用使用相对资源路径，可部署在 `/apps/three-body/` 子目录。

## 自动发布

线上地址：<https://itsusinn.github.io/apps/three-body/>。

`main` 构建成功后，`deploy-website` 任务即时触发 `Itsusinn/itsusinn.github.io` 的 `deploy.yml`。网站工作流解析三体 `main` 的最新提交，调用本仓库的可复用构建流程，再将产物放入网站的 `site/apps/three-body/`，校验整个网站并发布到 GitHub Pages。网站始终构建完整站点，博客和模拟应用一起发布。Pull request 只检查，不触发发布；网站调用本工作流时也不会再次触发网站，避免递归。

首次配置：

1. 创建 fine-grained personal access token，仅授权 `Itsusinn/itsusinn.github.io`，Repository permissions 选择 **Actions: Read and write**。
2. 在本仓库 **Settings → Secrets and variables → Actions** 添加名为 `WEBSITE_DEPLOY_TOKEN` 的 secret，填入该令牌。
3. 网站的 Pages Source 设为 **GitHub Actions**，`github-pages` 环境允许 `master` 分支部署。

不要将令牌写入源码。缺少或过期的令牌会使通知任务明确失败，但仍保留已通过的构建产物。配置好 secret 后可以重跑失败任务，或手动运行本仓库的 **Build WebGPU app**。也可手动运行网站的 **CI and Pages** 完成发布。

通知成功表示网站工作流已入队；实际发布结果请查看网站仓库的 Actions。线上 `/apps/three-body/version.json` 记录已发布的源码提交。GitHub 托管执行器的排队和构建会带来延迟。

## 操作

- 顶部 **Scene** 下拉菜单切换已注册场景，现有场景命名为 `2d-345-threebody`，也是目前唯一的默认场景。切换在下一个 Bevy `Update` 生效，释放旧场景的模拟、草稿和历史，新场景从初始暂停状态开始；重复选择当前场景不会重置。隐藏 Parameters 不会隐藏场景入口。
- **Run / Pause** 或空格：运行、暂停。启动时暂停，保留经典初始构型。
- **Step**：暂停时请求推进一个 Max dt 的模拟时间；误差控制可能拆成多个子步，遇到工作量上限时只推进已完成的部分。
- **Reset** 或 R：恢复当前已应用的初始条件，并清空轨迹与误差历史。
- **Speed / Max dt**：实时修改时间倍率与积分步长上限。
- **Rel. tol / Abs. tol**：实时修改相对、绝对误差容差。DOP853 始终启用自适应误差控制；容差约束估计的局部误差，不保证整个轨迹的全局误差。
- **Initial conditions**：修改三颗天体的质量、位置、速度及 G、软化长度后，点击 **Apply & restart** 生效。修改这些参数不会悄悄改变正在运行的系统。
- **Restore classic 3-4-5**：恢复经典质量、位置、零速度、G=1、零软化。
- 拖动视图平移、滚轮缩放、双击或 **Fit all bodies** 适配视野。**Auto frame** 持续跟随所有天体。
- 可以调节轨迹长度、透明度、天体显示大小、网格。天体图标大小仅用于显示，不是碰撞半径。
- 窄窗口可用顶部 **Parameters** 隐藏或显示参数面板。

## 初始条件与数值模型

| 质量 | x | y | vx | vy |
|---|---|---|---|---|
| 3 | 1 | 3 | 0 | 0 |
| 4 | -2 | -1 | 0 | 0 |
| 5 | 1 | -1 | 0 | 0 |

每颗天体的质量等于对边边长，质心为原点。默认 G=1，使用无量纲单位。初始总能量为 `-769/60`，总动量与角动量为零。

引力加速度为 `a_i = G Σ m_j (r_j-r_i) / (|r_j-r_i|²+ε²)^(3/2)`，势能使用同一软化模型。默认 ε=0，为牛顿点质量模型；ε>0 为 Plummer 软化，会改变近距离动力学。天体不合并，不做弹性碰撞。

- CPU/WASM 使用 `ode_solvers::Dop853<f64>`，积分 12 个位置/速度分量。DOP853 是 8 阶显式 Runge–Kutta 方法，带 5/3 阶嵌入误差估计；不是辛积分器，也不是 IAS15。
- 默认最大步长 0.05，相对容差 `1e-11`、绝对容差 `1e-13`。求解器按 `atol + rtol * max(|y_old|, |y_new|)` 缩放误差，自动接受或拒绝步骤。容差不是全局精度承诺；近距离相遇、浮点舍入和混沌仍会放大误差，长期结果应以收敛性和独立参考验证。
- 每帧最多 2048 次尝试，包含被拒绝的步骤；界面子步数显示已接受的步骤。达到上限后保留最后有效状态，降低实际模拟速度并显示提示，不增大步长追赶时间。切回后台标签页后不补算全部逝去时间；单帧墙钟时间最多计入 0.05 秒。
- 每帧按目标时刻调用求解器，继承上一帧最后接受的步长作为初始猜测。使用 Sparse 输出读取真实接受的状态；每帧输出长度由尝试次数限制，避免历史无限增长。
- 界面显示 CPU 双精度计算的实际模拟时间、总能量、相对能量误差、总动量、角动量、当前最小距离和子步数。诊断每帧更新，不再从 GPU 异步读取。
- 当时间或步长低于 f64 实际可推进范围、求解器检测到刚性，或状态超出数值/显示范围时停止积分并提示；保留最后有效状态，重置后恢复。零软化下重合的初始位置会被拒绝。
- 每颗天体保留最近 4096 个轨迹采样点，重置时记录初始位置，每个有推进的渲染帧再采样一次；轨迹采样时长随帧率与时间倍率改变。暂停不会追加重复轨迹。物理仅在 Bevy `Update` 中推进一次，egui 多轮布局或重复渲染回调均不会再次积分。界面操作在下一个 `Update` 生效，不使用固定时间调度补算后台时间。

## 验证

```powershell
cargo fmt --check
cargo clippy --target wasm32-unknown-unknown -- -D warnings
cargo test --jobs 2 -- --nocapture
# 仅物理测试，不需要 GPU adapter：
cargo test --jobs 2 simulation:: -- --nocapture
```

物理测试在 t=1 将 DOP853 位置与速度和独立 f64 RK4 参考计算对照，并检查拉格朗日等边三体解析轨道。经典初值延长至 t=10，检查近距离相遇的能量、动量与角动量，以及收紧容差、改变帧间隔后的收敛性。另有暂停、单步、重置、工作量上限、软化后的重合初值和错误状态测试。独立 RK4 参考只存在于测试中。Bevy 调度测试验证暂停、单步、重置、长帧截断、错误恢复和轨迹环绕。渲染测试使用可用的 GPU adapter，验证 WGSL 管线、bevy_egui 回调、重复上传、重置和离屏像素回读。

2026-10-15 Windows 本地验证：经典初值至 t=10，按 0.05 的帧间隔测得最大相对能量漂移约 `9.59e-10`；容差收紧至 `rtol=1e-13, atol=1e-15` 后约 `1.03e-11`。两次运行最终位置/速度分量最大差约 `9.37e-8`；严格容差下改为 0.017 帧间隔，最终最大差约 `3.00e-10`。这是指定初值、时间范围和采样方式的实测，不能推广为任意初值的精度保证。

## 代码结构

- `src/main.rs`：Bevy 窗口、WebGPU、相机和系统注册。
- `src/scenes/mod.rs`：公共场景接口、注册表、场景选择界面和切换生命周期。新增场景实现 `Scene` 并注册唯一稳定 ID 与工厂；工厂创建暂停状态，物理推进放在 `advance`，界面与渲染提交放在 `ui`。场景独立持有状态，不要求复用三体模型。
- `src/scenes/three_body.rs`：`2d-345-threebody` 场景，持有并调度现有三体界面、物理与轨迹状态。
- `src/scenes/style.rs`：所有场景共用的 egui 外观。
- `src/scenes/tests.rs`：场景切换、退出释放、重复选择、多轮布局和窄屏验证。
- `src/app.rs`：三体场景的界面状态，以及 egui 参数、交互、诊断图表和视野控制。
- `src/runtime.rs`：由场景在 Bevy `Update` 中调用的物理推进、CPU 轨迹环形缓冲和不可变显示快照。
- `src/simulation.rs`：CPU f64 引力方程、DOP853 积分、工作量限制和守恒量诊断。
- `src/gpu.rs`：Bevy 渲染世界中的 GPU buffers、渲染管线和 bevy_egui 回调；仅上传和绘制，不持有求解器。
- `src/model.rs`：双精度初始条件、输入验证、诊断与显示数据的显式字节布局。
- `src/shaders/render.wgsl`：网格、轨迹条带、天体光晕。
- `src/simulation_tests.rs`：独立 RK4、解析轨道、近距离相遇与运行控制验证。
- `src/runtime_tests.rs`：无窗口、无 GPU 的 Bevy 调度和轨迹历史验证。
- `src/gpu_tests.rs`：渲染与 bevy_egui 回调集成验证。

后续新增或替换技术栈组件前，须先询问项目使用者。
