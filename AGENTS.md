# physics-simulation · 物理实验室

浏览器中的交互式物理实验室，包含二维与三维三体、双摆、耦合弹簧、开普勒轨道和滑块碰撞。使用经确认的 **Rust / WASM + Bevy 0.20.0 + bevy_egui 0.43.1 + wgpu + Trunk**。物理计算位于 CPU，五个 ODE 场景默认使用 **Rust `ode_solvers` 0.6.2 的 DOP853 + f64**，支持运行时切换积分算法，积分状态、初始条件和守恒量诊断全部保留双精度，只在上传显示数据时转换为 f32。滑块场景使用用户于 2026-10-17 确认替换的 Rust 原生 f64 事件驱动解析碰撞求解器，其模型与数值边界见后文。WebGPU 负责渲染；没有 JavaScript 物理引擎或额外前端框架。

## 运行

需要 Rust 1.97.1+、`wasm32-unknown-unknown` 目标、可通过 `python` 调用的 Python 3.9+，以及支持 WebGPU 的浏览器。使用 localhost 或 HTTPS，不能直接打开 HTML 文件。

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

启动页面显示 WASM 下载百分比与已下载大小，随后显示初始化状态，场景界面就绪后隐藏；加载失败时显示错误与重试按钮。`loading.mjs` 接入 Trunk initializer。`scripts/fix-wasm-progress.py` 使用 Python 标准库，在 post_build 阶段将 Trunk 0.21.14 记录的优化前大小校正为最终 WASM 大小，保证压缩传输时的进度也正确。该脚本校验生成模板，Trunk 升级后若模板变化需同步调整。

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

源码仓库：<https://github.com/Itsusinn/physics-simulation>。

推送到 `main` 或创建 pull request 后，GitHub Actions 使用 `rust-toolchain.toml` 固定的 Rust 1.98.1 和经过 SHA-256 校验的 Trunk 0.21.14，执行格式检查、WASM Clippy 检查和锁定依赖的 release 构建。生成的 `physics-simulation-web` artifact 包含完整 HTML / JS / WASM 和记录源码提交的 `version.json`，保留 14 天。工作流也支持手动运行和由网站工作流调用。

CI 同时校验 Binaryen 133 的下载摘要，并在 Actions summary 与产物 `size-report.json` 中记录 WASM 原始字节数和 gzip-9 估算。WASM 总体积上限为 20,000,000 字节；超过时构建失败。gzip 数值用于对比，不代表服务器实际传输大小，也不会生成用于部署的 `.gz` 文件。

CI 目前执行上述 WASM 检查和构建；原生物理与渲染测试在本地执行，其中仅渲染测试需要 GPU adapter。应用使用相对资源路径，可部署在 `/apps/physics-simulation/` 子目录。

## 自动发布

线上地址：<https://itsusinn.github.io/apps/physics-simulation/>。

原 `/apps/three-body/` 入口由网站保留跳转到新地址。

`main` 构建成功后，`deploy-website` 任务即时触发 `Itsusinn/itsusinn.github.io` 的 `deploy.yml`。网站工作流解析物理实验室 `main` 的最新提交，调用本仓库的可复用构建流程，再将产物放入网站的 `site/apps/physics-simulation/`，校验整个网站并发布到 GitHub Pages。网站始终构建完整站点，博客和模拟应用一起发布。Pull request 只检查，不触发发布；网站调用本工作流时也不会再次触发网站，避免递归。

首次配置：

1. 创建 fine-grained personal access token，仅授权 `Itsusinn/itsusinn.github.io`，Repository permissions 选择 **Actions: Read and write**。
2. 在本仓库 **Settings → Secrets and variables → Actions** 添加名为 `WEBSITE_DEPLOY_TOKEN` 的 secret，填入该令牌。
3. 网站的 Pages Source 设为 **GitHub Actions**，`github-pages` 环境允许 `master` 分支部署。

不要将令牌写入源码。缺少或过期的令牌会使通知任务明确失败，但仍保留已通过的构建产物。配置好 secret 后可以重跑失败任务，或手动运行本仓库的 **Build WebGPU app**。也可手动运行网站的 **CI and Pages** 完成发布。

通知成功表示网站工作流已入队；实际发布结果请查看网站仓库的 Actions。线上 `/apps/physics-simulation/version.json` 记录已发布的源码提交。GitHub 托管执行器的排队和构建会带来延迟。

## 操作

- 顶部 **Scene** 下拉菜单切换 `2d-345-threebody`（默认）、`3d-threebody`、`2d-double-pendulum`、`1d-coupled-springs` 、`2d-kepler-orbit` 与 `2d-block-collisions`。切换在下一个 Bevy `Update` 生效，释放旧场景的模拟、草稿和历史，新场景从初始暂停状态开始；重复选择当前场景不会重置。隐藏 Parameters 不会隐藏场景入口。
- **Run / Pause** 或空格：运行、暂停。启动时暂停，保留经典初始构型。
- **Step**：暂停时请求推进一个 Max dt 的模拟时间；误差控制可能拆成多个子步，遇到工作量上限时只推进已完成的部分。
- **Reset** 或 R：恢复当前已应用的初始条件，并清空轨迹与误差历史。
- **Speed / Max dt**：实时修改时间倍率与积分步长上限。
- **Rel. tol / Abs. tol**：DOP853 与 DOPRI5 使用自适应误差控制；容差约束估计的局部误差，不保证整个轨迹的全局误差。RK4 与 Velocity Verlet 不使用容差，界面禁用这两个控件；需缩小 Max dt 检查收敛。
- **Initial conditions**：修改三颗天体的质量、位置、速度及 G、软化长度后，点击 **Apply & restart** 生效。修改这些参数不会悄悄改变正在运行的系统。
- **Restore classic 3-4-5**：恢复经典质量、位置、零速度、G=1、零软化。
- 拖动视图平移、滚轮缩放、双击或 **Fit all bodies** 适配视野。**Auto frame** 持续跟随所有天体。
- 可以调节轨迹长度、透明度、天体显示大小、网格。天体图标大小仅用于显示，不是碰撞半径。
- 窄窗口可用顶部 **Parameters** 隐藏或显示参数面板。

## 算法热切换

顶部 **Algorithm** 可在运行或暂停时切换已编译的算法，下一次 Bevy Update 生效，保留当前 f64 状态、时间、能量基准、初值草稿、轨迹和运行/暂停状态，仅清除旧算法的步长猜测。Reset / 预设保留当前算法；切换场景采用新场景的首个推荐算法。已有数值错误不会被算法切换清除，仍需 Reset 恢复。不是运行时下载或动态加载外部代码。

`AlgorithmDefinition.supported_scenes` 声明兼容场景 ID；`SceneDefinition.recommended_algorithms` 是有序推荐列表，首项作为进入场景时的默认值。选择器仅列出兼容算法，并标注 recommended；悬停 **About algorithms** 查看算法说明、支持场景和场景推荐。`Scene::set_algorithm` 与积分入口均检查兼容性。

| ID | 算法 | 支持场景 |
| --- | --- | --- |
| `dop853` | 8 阶自适应 DOP853，5/3 误差估计 | 全部五个 |
| `dopri5` | 自适应 Dormand–Prince 5(4) | 全部五个 |
| `rk4` | 经典四阶 Runge–Kutta | 全部五个 |
| `velocity-verlet` | 二阶 kick–drift–kick | 二维/三维三体、耦合弹簧、开普勒 |

三体和双摆推荐 DOP853、DOPRI5；弹簧与开普勒推荐 DOP853、Velocity Verlet。所有场景默认 DOP853。DOP853 / DOPRI5 复用现有 `ode_solvers` 0.6.2；RK4 / Verlet 在 Rust 内实现，没有新增依赖或更换技术栈。当前双摆的角加速度依赖角速度，不适用这个 Verlet 后端。

RK4 / Verlet 按 Max dt 推进，帧末可缩短步长，不提供自适应误差控制。Verlet 在可分离模型和固定步长下具有辛结构，但应用不保证跨渲染帧步长恒定；近距离相遇尤其需要小步长及收敛验证。所有算法继续使用 f64、每帧最多 2048 次尝试、最后有效状态保护和不追赶后台时间的策略。DOPRI5 的库内部计数边界不同，公共封装将预算减一传入以保持同样的实际尝试上限。

新增算法需要在 `Algorithm`、`ALGORITHMS` 和积分分派中登记并补充测试；新增场景须填写推荐列表，并将其 ID 加入兼容算法的支持列表。测试校验双向元数据、每个后端的解析解/独立参考、固定步长收敛阶、状态连续性、重置、场景切换及窄屏布局。

## 初始条件与数值模型

以下经典初值与二维数值结果适用于 `2d-345-threebody`。三维场景的区别见后面的「三维场景」。

| 质量 | x | y | vx | vy |
|---|---|---|---|---|
| 3 | 1 | 3 | 0 | 0 |
| 4 | -2 | -1 | 0 | 0 |
| 5 | 1 | -1 | 0 | 0 |

每颗天体的质量等于对边边长，质心为原点。默认 G=1，使用无量纲单位。初始总能量为 `-769/60`，总动量与角动量为零。

引力加速度为 `a_i = G Σ m_j (r_j-r_i) / (|r_j-r_i|²+ε²)^(3/2)`，势能使用同一软化模型。默认 ε=0，为牛顿点质量模型；ε>0 为 Plummer 软化，会改变近距离动力学。天体不合并，不做弹性碰撞。

- CPU/WASM 默认使用 `ode_solvers::Dop853<f64>`，积分 12 个位置/速度分量。DOP853 是 8 阶显式 Runge–Kutta 方法，带 5/3 阶嵌入误差估计；不是辛积分器，也不是 IAS15。
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

物理测试在 t=1 将 DOP853 位置与速度和独立 f64 RK4 参考计算对照，并检查拉格朗日等边三体解析轨道。经典初值延长至 t=10，检查近距离相遇的能量、动量与角动量，以及收紧容差、改变帧间隔后的收敛性。另有暂停、单步、重置、工作量上限、软化后的重合初值和错误状态测试。独立 RK4 参考只存在于测试中，不复用新增的生产 RK4 后端。Bevy 调度测试验证暂停、单步、重置、长帧截断、错误恢复和轨迹环绕。渲染测试使用可用的 GPU adapter，验证 WGSL 管线、bevy_egui 回调、重复上传、重置和离屏像素回读。

2026-10-15 Windows 本地验证：经典初值至 t=10，按 0.05 的帧间隔测得最大相对能量漂移约 `9.59e-10`；容差收紧至 `rtol=1e-13, atol=1e-15` 后约 `1.03e-11`。两次运行最终位置/速度分量最大差约 `9.37e-8`；严格容差下改为 0.017 帧间隔，最终最大差约 `3.00e-10`。这是指定初值、时间范围和采样方式的实测，不能推广为任意初值的精度保证。

## 三维场景

`3d-threebody` 使用同一技术栈，独立持有三维模型、可切换 f64 积分器、界面与轨迹。每颗天体有 x/y/z 位置和 vx/vy/vz 速度，共积分 18 个分量；引力与 Plummer 软化公式推广到三维，动量和角动量均显示三个分量。默认容差、步长、每帧 2048 次尝试、长帧截断和错误停止语义与二维一致。位置超出 ±1e12 的显示范围时保留最后有效状态并停止。

默认使用空间扰动的 3-4-5 初值（G=1、软化=0），保持质心和总动量为零：

| 质量 | x | y | z | vx | vy | vz |
|---|---|---|---|---|---|---|
| 3 | 1 | 3 | 0.4 | 0 | 0 | 0.18 |
| 4 | -2 | -1 | -0.45 | 0.12 | 0 | -0.1 |
| 5 | 1 | -1 | 0.12 | -0.096 | 0 | -0.028 |

**Restore spatial preset** 恢复此初值；**Classic 3-4-5 in XY plane** 恢复经典零速度平面初值，后者在无空间扰动时应始终保持平面运动。草稿参数点击 **Apply & restart** 后在下一次 Update 生效。每帧有推进时采样一次空间轨迹，最多 4096 点，暂停不重复采样。

拖动旋转、Shift 拖动或右键拖动平移、滚轮缩放、双击或 **Fit all bodies** 适配视野。**Reset camera** 重置观察方向，**Auto frame** 跟随所有天体。网格为 XY 平面，X/Y/Z 轴分别为红/绿/蓝色。球体尺寸仅是显示大小，不是碰撞半径。

三维绘制继续使用 bevy_egui 的 WebGPU 回调：透视矩阵、球体网格、空间轨迹和坐标网格在带深度缓冲的离屏目标中绘制，再合成到界面。没有新增依赖或启用 Bevy PBR。离屏分辨率按视窗物理像素设置，每边最大 4096，并保持宽高比。只有显示快照和相机使用 f32，物理状态和诊断始终为 f64。

测试包含独立三维 RK4 对照、倾斜等边解析轨道、与二维求解器的平面极限对照、三维守恒与收敛、暂停/重置/错误恢复/轨迹容量、往返场景切换和 GPU 深度遮挡/缩放/重入回读。2026-10-15 本地空间初值至 t=10：默认容差终点相对能量漂移 `5.02e-10`，严格容差 `3.62e-12`，两种容差终点位置/速度分量最大差 `3.56e-8`；严格容差下改用 0.017 帧间隔后最大差 `5.24e-10`。这些是指定初值和时间范围的实测，不是任意初值的精度保证。

## 经典力学场景

- `2d-double-pendulum`：均匀重力中的无质量刚杆与两个质点，角度由竖直向下起算，单位为弧度。提供大角度和小角度预设；可修改质量、杆长、重力、初始角度与角速度。以拉格朗日质量矩阵求解角加速度，势能零点为向下悬垂平衡位置，绘制两个摆球的轨迹。
- `1d-coupled-springs`：两质量块、相同劲度的两外侧弹簧与中间耦合弹簧。提供能量交换、同相和反相正常模预设；可修改质量、劲度、初始位移和速度。位移相对于平衡位置，支持耦合劲度为零。显示各振子及耦合弹簧能量、位移随实际模拟时间的曲线。弹簧间距和质点图标是示意，不做碰撞。
- `2d-kepler-orbit`：固定中心引力场中的测试粒子，`mu = G M`，不是两个相互加速的天体。提供椭圆、圆形和逃逸轨道预设；可修改 mu、初始位置与速度。能量和角动量为单位粒子质量的量，并显示偏心率。无软化；半径小于等于 `1e-6` 时停止并保留最后有效状态，这是数值保护边界，不是碰撞半径。

三个场景独立持有模型与状态，共用 `src/scenes/mechanics/` 内的控制界面和积分封装，使用共享可切换 f64 积分器（默认 DOP853）、默认 `rtol=1e-11` / `atol=1e-13`、最大步长 0.05、每帧 2048 次尝试。墙钟单帧截断到 0.05 秒后应用 Speed，预算耗尽时丢弃未计算时间。`advance` 是唯一积分入口，界面多轮绘制不推进物理。图元通过现有 bevy_egui WebGPU 渲染器绘制，没有新增依赖；最终显示投影才转换为 f32。

Run / Pause、Step、Reset、空格与 R 均可使用。初始参数草稿点击 Apply & restart 后生效；预设按钮在下一次 Update 应用并暂停，Reset 恢复最近已应用初值。拖动平移、滚轮缩放、双击或 Fit experiment 适配，Auto frame 包含当前显示历史。最多保留 4096 个逐推进帧采样，暂停不追加，重启清空。缩放能量误差使用 `(E-E0)/max(|K0|+|U0|, 1e-15)`，避免接近零总能量导致相对误差失去意义；不代表全局轨迹精度。

验证包括弹簧解析正常模与解耦极限、开普勒周期闭合与角动量、双摆独立显式方程 RK4 对照与收紧容差、所有预设至 t=20 的能量守恒、奇点停止和工作预算，以及暂停/单步/重置/错误恢复/草稿隔离/历史容量、所有场景往返切换和窄宽屏多轮网格化。聚焦测试：`cargo test --jobs 2 scenes::mechanics:: -- --nocapture`。

## 事件驱动滑块碰撞场景

2026-10-17 用户已确认将原 Avian PR #1070 求解器替换为 **Rust 原生 f64 事件驱动解析算法**，移除 Avian 及其碰撞依赖。Rust / WASM、Bevy、egui 与渲染技术栈继续使用。原有五个 ODE 场景仍使用各自的可切换积分器。

`2d-block-collisions`：固定墙面 x=0，横轨道绘制到视窗边缘。左侧物块默认质量 1、边长 0.7、中心 x=2、初始静止；右侧物块默认质量 100、边长 1.4、中心 x=5、初速度 -1。仅水平运动，无重力、摩擦与旋转；图形尺寸定义实际接触面。模型假定刚体与瞬时碰撞，不描述形变、接触持续时间及材料相关恢复系数。

两个质量可独立输入正数、小数及大值，质量、倒数、归一化质量比和初始动能需在 f64 正规有限数范围内。来向速度 0.1–3，恢复系数 0–1，默认 1。Apply & restart 在下次 Update 应用并暂停，Reset 恢复最近已应用设置。Run/Pause、Step（0.05 模拟秒）、空格、R、平移、缩放和自动适配保持。总碰撞次数为物块/物块、左物块/墙的分项之和，隐藏 Parameters 仍可见；暂停保持，重置归零，持续接触不重复计数。

碰撞间为 f64 匀速直线运动。分别以墙面间隙除以来向速度、物块间隙除以相对接近速度计算接触时刻，处理最早的事件。令碰前速度为 u、v，质量分数 a=m_left/(m_left+m_right)、b=m_right/(m_left+m_right)，恢复系数 e，则物块碰后速度为 `u' = u-(1+e)b(u-v)`、`v' = v+(1+e)a(u-v)`；墙碰撞为 `u'=-eu`。计算质量分数前先用较大质量缩放，避免质量和溢出。质量、速度、间隙、碰撞时刻与诊断均为 f64，仅显示投影转 f32。

物理状态记录两个表面间隙，以上次碰撞为自由飞行锚点；渲染帧只采样轨迹，不改变锚点，避免逐帧重算引入接触误差。无固定碰撞步长、穿透修正冲量或低速反弹阈值。每帧墙钟截断至 0.05 秒后乘 Speed，最多处理 2048 次碰撞；耗尽时保留最后已完成事件，丢弃未算时间并提示，不跳过碰撞或后台追赶。`advance` 是唯一推进入口，UI 多轮绘制不推进。切换场景释放求解器。此场景 `Scene::algorithm()` 返回 None，ODE 推荐列表为空，顶部 `fixed_solver` 显示事件驱动 f64 专用求解器。

诊断分别显示动能 K、累计物理耗散 Q、相对动能变化与数值残差 `(K+Q-K0)/K0`。物块碰撞耗散 `0.5 μ(1-e²)(u-v)²`（μ 为约化质量）；墙碰撞耗散 `0.5 m_left(1-e²)u²`。固定墙会交换物块系统的动量。e=0 时接触物块同速运动，在墙面共同停止，不制造额外碰撞。

常恢复系数瞬时模型存在非弹性塌缩：有限时间内可能发生无限次碰撞。下一碰撞时刻小于 f64 可推进范围时明确停止并保留最后有效状态，不使用人为静止阈值或伪造最终有限计数。完全同时的弹性/部分弹性墙与物块碰撞也停止提示，因为二元恢复系数不能唯一规定多点材料响应。极端质量比即使 e=1 也可能耗尽数值分辨率，f64 不保证任意质量比的严格计数。非有限状态及超过 ±1e12 的位置/速度停止，Reset 恢复。

测试覆盖解析碰撞时刻、碰后速度、动量及能量收支、e=0 持续接触、非弹性塌缩、事件预算、帧划分、生命周期与窄屏。独立 Galperin 几何公式 `ceil(pi / atan(sqrt(m_left/m_right))) - 1` 验证质量比 1、100、10⁴、10⁶、10⁸ 的总次数 3、31、314、3141、31415。2026-10-17 Windows 实测默认比值相对能量漂移约 4.4e-16，31415 次情形约 2.3e-12；这是特定初值与采样的实测，不是普遍精度承诺，最后一次慢速碰撞可能远晚于密集碰撞阶段。聚焦检查：`cargo test --locked --jobs 2 scenes::blocks:: -- --nocapture`。

历史 Avian 版本在 2026-10-15 的 Windows release 体积为原始 18,638,724 字节、gzip-9 估算 5,965,105 字节；移除依赖后的发布体积需在对应提交上重新测量。CI 原始体积预算仍为 20,000,000 字节。

## 代码结构

- `src/main.rs`：Bevy 窗口、WebGPU、相机和系统注册。
- `src/scenes/mod.rs`：公共场景接口、注册表、场景选择界面和切换生命周期。新增场景实现 `Scene` 并注册唯一稳定 ID 与工厂；工厂创建暂停状态，物理推进放在 `advance`，界面与渲染提交放在 `ui`。场景独立持有状态，不要求复用三体模型。
- `src/scenes/three_body.rs`：`2d-345-threebody` 场景，持有并调度现有三体界面、物理与轨迹状态。
- `src/scenes/three_d/`：`3d-threebody` 场景；`mod.rs` 管理界面与运行状态，`model.rs` / `physics.rs` 负责三维 f64 模型与积分，`camera.rs` / `render.rs` / `render.wgsl` 负责透视相机和深度绘制。对应测试与场景代码放在同一目录。
- `src/scenes/mechanics/`：双摆、耦合弹簧与开普勒场景；`physics.rs` 负责独立力学模型和有界 f64 可切换积分，`mod.rs` 负责控制、相机、轨迹和 egui 图元，`physics_tests.rs` / `tests.rs` 验证物理及生命周期。
- `src/scenes/blocks/`：f64 事件驱动滑块碰撞场景、独立求解器、控制、绘制及碰撞/生命周期测试。
- `src/scenes/style.rs`：所有场景共用的 egui 外观。
- `src/scenes/tests.rs`：场景切换、退出释放、重复选择、多轮布局和窄屏验证。
- `src/app.rs`：三体场景的界面状态，以及 egui 参数、交互、诊断图表和视野控制。
- `src/runtime.rs`：由场景在 Bevy `Update` 中调用的物理推进、CPU 轨迹环形缓冲和不可变显示快照。
- `src/simulation.rs`：CPU f64 引力方程、可切换积分、工作量限制和守恒量诊断。
- `src/gpu.rs`：Bevy 渲染世界中的 GPU buffers、渲染管线和 bevy_egui 回调；仅上传和绘制，不持有求解器。
- `src/integrators.rs`：算法注册表 `ALGORITHMS`、兼容场景元数据、DOP853 / DOPRI5 / RK4 / Velocity Verlet 后端与公共容差界面。`src/integrators/tests.rs` 验证解析解、收敛阶、长期能量行为及预算。
- `src/model.rs`：双精度初始条件、输入验证、诊断与显示数据的显式字节布局。
- `src/shaders/render.wgsl`：网格、轨迹条带、天体光晕。
- `src/simulation_tests.rs`：独立 RK4、解析轨道、近距离相遇与运行控制验证。
- `src/runtime_tests.rs`：无窗口、无 GPU 的 Bevy 调度和轨迹历史验证。
- `src/gpu_tests.rs`：渲染与 bevy_egui 回调集成验证。

后续新增或替换技术栈组件前，须先询问项目使用者。
