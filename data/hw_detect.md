

**QSV（Intel Quick Sync Video）**、**VAAPI** 和 **CUDA（NVIDIA GPU 加速）** 三种硬件加速方式的检测方法，通过检查硬件、驱动和系统环境来判断支持性：

---

### **1. QSV（Intel Quick Sync Video）检测**
QSV 是 Intel 核显的硬件编解码技术，依赖 Intel 驱动和媒体 SDK。

#### **步骤 1：检查 Intel 核显驱动**
```bash
lspci -nn | grep -Ei "VGA|Display" | grep -i Intel
```
输出示例：
```bash
00:02.0 VGA compatible controller [0300]: Intel Corporation HD Graphics 620 [8086:5916] (rev 02)
```

#### **步骤 2：验证内核模块加载**
```bash
lsmod | grep i915
```
若输出包含 `i915`，说明 Intel 核显驱动已加载。

#### **步骤 3：检查设备节点**
```bash
ls -l /dev/dri/
```
正常输出应包含 `card0` 和 `renderD128`：
```bash
crw-rw---- 1 root video 226, 0  Aug 1 10:00 card0
crw-rw---- 1 root render 226, 128 Aug 1 10:00 renderD128
```

#### **步骤 4：检查 VAAPI/QSV 支持**
安装 `vainfo` 工具（如未安装）：
```bash
sudo apt-get install vainfo  # Debian/Ubuntu
sudo dnf install vainfo      # Fedora
```
运行：
```bash
vainfo
```
输出示例（显示支持的编解码）：
```bash
VAProfileH264High               : VAEntrypointEncSlice      # 支持 H.264 编码
VAProfileHEVCMain               : VAEntrypointVLD          # 支持 HEVC 解码
```

#### **步骤 5：确认 Intel 媒体驱动**
检查是否安装 Intel 媒体驱动库：
```bash
ls /usr/lib/x86_64-linux-gnu/libmfx.so*  # 检查 Media SDK 库是否存在
```
若存在 `libmfx.so.1` 或类似文件，说明系统支持 QSV。

---

### **2. VAAPI 检测**
VAAPI 是通用的视频加速接口，支持 Intel、AMD 和部分 NVIDIA 显卡。

#### **步骤 1：检查显卡支持**
```bash
lspci -nn | grep -Ei "VGA|Display"
```
根据输出判断显卡类型：
• **Intel**：支持 VAAPI（需 `i915` 驱动）
• **AMD**：支持 VAAPI（需 `amdgpu` 驱动）
• **NVIDIA**：需安装 `nvidia-vaapi-driver`（较新驱动）

#### **步骤 2：验证驱动加载**
```bash
lsmod | grep -E "i915|amdgpu|nvidia"
```
根据显卡类型检查驱动模块是否加载。

#### **步骤 3：运行 `vainfo`**
```bash
vainfo
```
输出示例：
```bash
# Intel 核显
VAProfileH264High               : VAEntrypointEncSlice
VAProfileHEVCMain               : VAEntrypointVLD

# NVIDIA 显卡（需安装 nvidia-vaapi-driver）
VAProfileH264High               : VAEntrypointEncSlice
VAProfileHEVCMain               : VAEntrypointVLD

# AMD 显卡
VAProfileH264High               : VAEntrypointVLD
```

#### **步骤 4：检查权限**
确保用户属于 `video` 和 `render` 组：
```bash
groups | grep -E "video|render"
```
若无输出，将用户加入组：
```bash
sudo usermod -aG video,render $USER
```

---

### **3. CUDA 检测**
CUDA 是 NVIDIA 的 GPU 计算加速技术，需安装 NVIDIA 驱动和 CUDA Toolkit。

#### **步骤 1：检查 NVIDIA 显卡**
```bash
lspci -nn | grep -i NVIDIA
```
输出示例：
```bash
01:00.0 VGA compatible controller [0300]: NVIDIA Corporation GP106 [GeForce GTX 1060] [10de:1b03] (rev a1)
```

#### **步骤 2：验证 NVIDIA 驱动**
```bash
lsmod | grep nvidia
```
输出应包含 `nvidia`、`nvidia_drm` 或 `nvidia_uvm`。

#### **步骤 3：检查 CUDA 库**
```bash
ls /usr/lib/x86_64-linux-gnu/libcuda*  # 检查 CUDA 运行时库
ls /usr/local/cuda/                   # 检查 CUDA Toolkit 安装路径
```

#### **步骤 4：运行 `nvidia-smi`**
```bash
nvidia-smi
```
输出示例（显示 GPU 状态和 CUDA 版本）：
```bash
+-----------------------------------------------------------------------------+
| NVIDIA-SMI 525.60.13    Driver Version: 525.60.13    CUDA Version: 12.0     |
|-------------------------------+----------------------+----------------------+
```

#### **步骤 5：编译 CUDA 测试程序**
创建 `cuda_test.cu`：
```cuda
#include <stdio.h>

__global__ void hello() {
    printf("Hello from GPU!\n");
}

int main() {
    hello<<<1,1>>>();
    cudaDeviceSynchronize();
    return 0;
}
```
编译并运行：
```bash
nvcc cuda_test.cu -o cuda_test && ./cuda_test
```
若输出 `Hello from GPU!`，说明 CUDA 支持正常。

---

### **总结表格**

| 加速技术 | 检测要点                                                                                   | 支持条件                                                                 |
|----------|------------------------------------------------------------------------------------------|--------------------------------------------------------------------------|
| **QSV**  | 1. Intel 核显存在且驱动 `i915` 加载<br>2. `/dev/dri/renderD128` 存在<br>3. `vainfo` 显示 Intel 编解码 | 需满足所有条件                                                           |
| **VAAPI**| 1. 显卡驱动加载（Intel/AMD/NVIDIA）<br>2. `vainfo` 显示支持的编解码<br>3. 用户属于 `video` 组          | 需满足所有条件                                                           |
| **CUDA** | 1. NVIDIA 显卡存在且驱动加载<br>2. `nvidia-smi` 显示 CUDA 版本<br>3. CUDA 库和工具链正常              | 需满足所有条件                                                           |

---

### **常见问题**
1. **权限问题**：若无法访问 `/dev/dri/renderD128`，将用户加入 `render` 和 `video` 组。
2. **驱动未安装**：  
   • Intel/AMD：安装开源驱动（如 `mesa-vdpau-drivers`）。  
   • NVIDIA：从官网下载驱动或使用 `nvidia-detect`（Debian/Ubuntu）。
3. **库缺失**：  
   • VAAPI：安装 `libva2`、`intel-media-va-driver`（Intel）或 `mesa-va-drivers`（AMD）。  
   • CUDA：安装 `nvidia-cuda-toolkit`。