//! wgpu 真管线 3D（Phase 2 探针）：egui PaintCallback 自绘棱柱网格。
//!
//! ## 管线设计
//!
//! - **离屏渲染 + blit 合成**：egui 主渲染通道只有 color 附件、无深度附件，
//!   真深度缓冲必须离屏——prepare 阶段把棱柱渲进离屏 color（Rgba8UnormSrgb，
//!   与 egui sRGB 目标恒等往返）+ depth（Depth32Float）纹理，paint 阶段以
//!   全屏三角形把离屏图 blit 进 egui 通道（viewport/scissor 对齐画布区）。
//! - **数据**：外环顶面扇形三角化 + 侧壁四边形展开（非索引三角面片，法线随面）；
//!   洞与线/点要素 spike 不画（正式管线事项，见 build_prism_mesh 注释）。
//! - **回退**：wgpu 不可用（无 GPU / 远程桌面 / 初始化失败）时 [`init`] 返回 false，
//!   壳层保持 scene3d 软件路径并终端提示一次。
//! - 已知边界（spike 取舍）：顶点缓冲每帧重建（正式管线缓存化）；多 wgpu 3D 视口
//!   共享离屏纹理（同帧多视口尺寸不同会互相重建——多视口为正式管线事项）。
//!
//! 纯函数（[`build_prism_mesh`]/[`orbit_mvp`]）配单测。

use std::sync::Mutex;

use eframe::egui;
use egui_wgpu::wgpu::{self, util::DeviceExt};

/// WGSL 着色器（棱柱 Lambert 光照 + 全屏 blit 两入口）。
const SHADER: &str = r#"
struct Uniforms {
    mvp: mat4x4<f32>,
    light: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;

struct Vin {
    @location(0) pos: vec3<f32>,
    @location(1) nrm: vec3<f32>,
    @location(2) col: vec4<f32>,
};
struct Vout {
    @builtin(position) pos: vec4<f32>,
    @location(0) nrm: vec3<f32>,
    @location(1) col: vec4<f32>,
};
@vertex
fn vs_prism(v: Vin) -> Vout {
    var o: Vout;
    o.pos = u.mvp * vec4<f32>(v.pos, 1.0);
    o.nrm = v.nrm;
    o.col = v.col;
    return o;
}
@fragment
fn fs_prism(v: Vout) -> @location(0) vec4<f32> {
    let l = normalize(u.light.xyz);
    // 两侧同光（abs）：spike 不区分内外法线朝向，保证侧壁全部可读。
    let d = abs(dot(normalize(v.nrm), l));
    let shade = 0.35 + 0.65 * d;
    return vec4<f32>(v.col.rgb * shade, v.col.a);
}

struct BlitOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex
fn vs_blit(@builtin(vertex_index) i: u32) -> BlitOut {
    // 全屏三角形：(-1,-1) (3,-1) (-1,3)。
    var o: BlitOut;
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    o.pos = vec4<f32>(p, 0.0, 1.0);
    o.uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    return o;
}
@group(0) @binding(0) var t_scene: texture_2d<f32>;
@group(0) @binding(1) var s_scene: sampler;
@fragment
fn fs_blit(v: BlitOut) -> @location(0) vec4<f32> {
    return textureSample(t_scene, s_scene, v.uv);
}
"#;

/// 棱柱顶点（位置/法线/颜色；bytemuck 直通顶点缓冲）。
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PrismVertex {
    /// 世界坐标（x 东、y 高、z 南——数据 y 翻转保持北向上）。
    pub pos: [f32; 3],
    /// 面法线。
    pub nrm: [f32; 3],
    /// 颜色（直线 sRGB 字节换算 0..1）。
    pub col: [f32; 4],
}

/// 棱柱部件（外环数据坐标（不含闭合重复点），高度（数据单位），颜色 RGBA 0..1）。
pub type PrismPart = (Vec<[f64; 2]>, f32, [f32; 4]);

/// 棱柱网格顶点（非索引三角面片；纯函数）。
///
/// `parts` 见 [`PrismPart`]。
/// 顶面扇形三角化（凸/简单多边形适用；凹多边形三角化与洞内环为正式管线事项——
/// spike 按扇形处理，与软件路径的 egui 凸多边形近似同级近似）。
/// `center`/`scale`：数据坐标归一化到世界系（跨度 → 约 2 个世界单位）；
/// `hscale`：高度 → 世界单位。
pub fn build_prism_mesh(
    parts: &[PrismPart],
    center: (f64, f64),
    scale: f64,
    hscale: f32,
) -> Vec<PrismVertex> {
    let mut out = Vec::new();
    for (ring, height, col) in parts {
        let n = ring.len();
        if n < 3 {
            continue;
        }
        let top = *height * hscale;
        let w = |p: [f64; 2], y: f32| {
            [
                ((p[0] - center.0) * scale) as f32,
                y,
                ((center.1 - p[1]) * scale) as f32, // 数据 y 翻转（北向上）
            ]
        };
        // 顶面（法线 +y；扇形三角化）。
        for i in 1..n - 1 {
            for p in [ring[0], ring[i], ring[i + 1]] {
                out.push(PrismVertex {
                    pos: w(p, top),
                    nrm: [0.0, 1.0, 0.0],
                    col: *col,
                });
            }
        }
        // 侧壁（每边两个三角；水平外法线）。
        for i in 0..n {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            let (wa0, wb0) = (w(a, 0.0), w(b, 0.0));
            let (wa1, wb1) = (w(a, top), w(b, top));
            let (dx, dz) = (wb0[0] - wa0[0], wb0[2] - wa0[2]);
            let len = (dx * dx + dz * dz).sqrt().max(1e-9);
            let nrm = [dz / len, 0.0, -dx / len];
            for p in [wa0, wb0, wb1, wa0, wb1, wa1] {
                out.push(PrismVertex {
                    pos: p,
                    nrm,
                    col: *col,
                });
            }
        }
    }
    out
}

/// 轨道相机 MVP（正交投影；yaw 绕 y 轴、pitch 绕 x 轴；scale = 半高世界单位，
/// aspect = 画布宽/高）。返回列主序 4×4（wgpu uniform 直通）。
pub fn orbit_mvp(yaw: f32, pitch: f32, scale: f32, aspect: f32) -> [[f32; 4]; 4] {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    // R = Ry(yaw)·Rx(pitch)（行主序推导）。
    let r = [
        [cy, sy * sp, sy * cp],
        [0.0, cp, -sp],
        [-sy, cy * sp, cy * cp],
    ];
    let (sx, syz) = (scale * aspect.max(1e-6), scale.max(1e-6));
    // 正交：x/=sx，y/=syz，z∈[-1000,1000] → [0,1]。
    let rows = [
        [r[0][0] / sx, r[0][1] / sx, r[0][2] / sx, 0.0],
        [r[1][0] / syz, r[1][1] / syz, r[1][2] / syz, 0.0],
        [r[2][0] / 2000.0, r[2][1] / 2000.0, r[2][2] / 2000.0, 0.5],
        [0.0, 0.0, 0.0, 1.0],
    ];
    // 行主序 → 列主序。
    let mut cols = [[0.0; 4]; 4];
    for (c, col) in cols.iter_mut().enumerate() {
        for (r_i, cell) in col.iter_mut().enumerate() {
            *cell = rows[r_i][c];
        }
    }
    cols
}

/// uniform（MVP + 光源方向）。
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    mvp: [[f32; 4]; 4],
    light: [f32; 4],
}

/// 离屏渲染目标（尺寸变化重建）。
struct Offscreen {
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    w: u32,
    h: u32,
}

/// 场景管线（启动时挂入 callback_resources；全局单例语义）。
struct ScenePipeline {
    pipeline: wgpu::RenderPipeline,
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    blit_pipeline: wgpu::RenderPipeline,
    blit_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    offscreen: Option<Offscreen>,
}

/// 帧资源（blit 绑定组；存于回调自身——多回调共存不互踩）。
struct FrameRes {
    blit_bind: wgpu::BindGroup,
}

/// 初始化 wgpu 3D 管线（eframe wgpu 后端可用时调用一次）；资源挂进
/// renderer 的 callback_resources。返回是否就绪（false = 保持软件路径）。
pub fn init(render_state: &egui_wgpu::RenderState) -> bool {
    let device = &render_state.device;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("kanyu-3d"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    // 场景管线（group 0 = uniform）。
    let scene_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("kanyu-3d-scene"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("kanyu-3d-uniform"),
        size: std::mem::size_of::<Uniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("kanyu-3d-bind"),
        layout: &scene_bgl,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform.as_entire_binding(),
        }],
    });
    let scene_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("kanyu-3d-scene-layout"),
        bind_group_layouts: &[Some(&scene_bgl)],
        immediate_size: 0,
    });
    let vbuf_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<PrismVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4],
    };
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("kanyu-3d-prism"),
        layout: Some(&scene_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_prism"),
            buffers: &[vbuf_layout],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_prism"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None, // spike 不做背面剔除（光照两侧同光）
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    // blit 管线（group 1 = 纹理 + 采样器；无顶点缓冲——全屏三角形由 vertex_index 生成）。
    let blit_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("kanyu-3d-blit-bgl"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let blit_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("kanyu-3d-blit-layout"),
        bind_group_layouts: &[Some(&blit_bgl)],
        immediate_size: 0,
    });
    let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("kanyu-3d-blit"),
        layout: Some(&blit_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_blit"),
            buffers: &[],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_blit"),
            targets: &[Some(wgpu::ColorTargetState {
                format: render_state.target_format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("kanyu-3d-sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    render_state
        .renderer
        .write()
        .callback_resources
        .insert(ScenePipeline {
            pipeline,
            uniform,
            bind,
            blit_pipeline,
            blit_bgl,
            sampler,
            offscreen: None,
        });
    true
}

/// 单帧绘制回调（顶点/MVP 构造时烘焙；帧资源经 Mutex 自持）。
struct FrameCallback {
    vertices: Vec<PrismVertex>,
    mvp: [[f32; 4]; 4],
    size_px: (u32, u32),
    frame_res: Mutex<Option<FrameRes>>,
}

impl egui_wgpu::CallbackTrait for FrameCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        if self.vertices.is_empty() || self.size_px.0 == 0 || self.size_px.1 == 0 {
            return Vec::new();
        }
        let Some(sp) = resources.get_mut::<ScenePipeline>() else {
            return Vec::new();
        };
        // 离屏纹理按视口物理尺寸重建。
        let need_new = sp
            .offscreen
            .as_ref()
            .is_none_or(|o| o.w != self.size_px.0 || o.h != self.size_px.1);
        if need_new {
            let make = |format: wgpu::TextureFormat, usage: wgpu::TextureUsages| {
                device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("kanyu-3d-offscreen"),
                        size: wgpu::Extent3d {
                            width: self.size_px.0,
                            height: self.size_px.1,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            };
            sp.offscreen = Some(Offscreen {
                color_view: make(
                    wgpu::TextureFormat::Rgba8UnormSrgb,
                    wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                ),
                depth_view: make(
                    wgpu::TextureFormat::Depth32Float,
                    wgpu::TextureUsages::RENDER_ATTACHMENT,
                ),
                w: self.size_px.0,
                h: self.size_px.1,
            });
        }
        let Some(off) = &sp.offscreen else {
            return Vec::new();
        };
        queue.write_buffer(
            &sp.uniform,
            0,
            bytemuck::bytes_of(&Uniforms {
                mvp: self.mvp,
                light: [-0.5, 1.0, -0.35, 0.0], // 左上前方来光
            }),
        );
        // 顶点缓冲（spike：每帧重建；正式管线按内容缓存）。
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("kanyu-3d-vbuf"),
            contents: bytemuck::cast_slice(&self.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        // 离屏渲染（真深度缓冲；清屏纯白与画布底色一致）。
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("kanyu-3d-scene-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &off.color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &off.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&sp.pipeline);
            pass.set_bind_group(0, &sp.bind, &[]);
            pass.set_vertex_buffer(0, vbuf.slice(..));
            pass.draw(0..self.vertices.len() as u32, 0..1);
        }
        let blit_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("kanyu-3d-blit-bind"),
            layout: &sp.blit_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&off.color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sp.sampler),
                },
            ],
        });
        *self.frame_res.lock().expect("帧资源锁") = Some(FrameRes { blit_bind });
        Vec::new()
    }

    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(sp) = resources.get::<ScenePipeline>() else {
            return;
        };
        let guard = self.frame_res.lock().expect("帧资源锁");
        let Some(fr) = guard.as_ref() else {
            return;
        };
        let vp = info.viewport_in_pixels();
        render_pass.set_viewport(
            vp.left_px as f32,
            vp.top_px as f32,
            vp.width_px as f32,
            vp.height_px as f32,
            0.0,
            1.0,
        );
        let clip = info.clip_rect_in_pixels();
        render_pass.set_scissor_rect(
            clip.left_px.max(0) as u32,
            clip.top_px.max(0) as u32,
            clip.width_px.max(0) as u32,
            clip.height_px.max(0) as u32,
        );
        render_pass.set_pipeline(&sp.blit_pipeline);
        render_pass.set_bind_group(0, &fr.blit_bind, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

/// 发射一帧 wgpu 3D 绘制（scene3d 三维态的后端分支）。
/// `parts`/`center`/`scale`/`hscale` 见 [`build_prism_mesh`]；`rect` 为画布区（逻辑点）。
#[allow(clippy::too_many_arguments)]
pub fn paint_scene(
    painter: &egui::Painter,
    rect: egui::Rect,
    pixels_per_point: f32,
    parts: &[PrismPart],
    center: (f64, f64),
    scale: f64,
    hscale: f32,
    yaw: f32,
    pitch: f32,
) {
    let vertices = build_prism_mesh(parts, center, scale, hscale);
    let aspect = rect.width() / rect.height().max(1.0);
    let cb = FrameCallback {
        vertices,
        mvp: orbit_mvp(yaw, pitch, 1.0, aspect),
        size_px: (
            (rect.width() * pixels_per_point).round().max(1.0) as u32,
            (rect.height() * pixels_per_point).round().max(1.0) as u32,
        ),
        frame_res: Mutex::new(None),
    };
    painter.add(egui_wgpu::Callback::new_paint_callback(rect, cb));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<[f64; 2]> {
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
    }

    #[test]
    fn prism_mesh_counts_and_normals() {
        // 单矩形：顶面 2 三角（6 顶点）+ 4 侧壁 × 2 三角（24 顶点）= 30。
        let mesh = build_prism_mesh(
            &[(square(), 10.0, [0.2, 0.5, 0.4, 1.0])],
            (0.5, 0.5),
            2.0,
            0.05,
        );
        assert_eq!(mesh.len(), 30);
        // 顶面法线 +y、高度 = 10×0.05 = 0.5。
        assert_eq!(mesh[0].nrm, [0.0, 1.0, 0.0]);
        assert!((mesh[0].pos[1] - 0.5).abs() < 1e-6);
        // 侧壁法线水平（y=0）；底面顶点 y=0。
        let side = &mesh[6];
        assert_eq!(side.nrm[1], 0.0);
        assert_eq!(side.pos[1], 0.0);
        // 世界系归一化：数据 (0,0) → x = (0-0.5)*2 = -1，z = (0.5-0)*2 = 1。
        assert!((mesh[6].pos[0] + 1.0).abs() < 1e-6);
        assert!((mesh[6].pos[2] - 1.0).abs() < 1e-6);
        // 空环/两点环不产生顶点。
        assert!(build_prism_mesh(
            &[(vec![[0.0, 0.0], [1.0, 1.0]], 1.0, [1.0; 4])],
            (0.0, 0.0),
            1.0,
            1.0
        )
        .is_empty());
    }

    #[test]
    fn orbit_mvp_identity_and_rotation() {
        // yaw=0/pitch=0/scale=1/aspect=1：x'=x，y'=y，z'=z/2000+0.5。
        let m = orbit_mvp(0.0, 0.0, 1.0, 1.0);
        // 列主序：v' = M·v；点 (1,0,0) → (1, 0, 0.5)。
        let x = m[0][0] + m[3][0]; // col0·1 + col3(w 列)
        let y = m[0][1] + m[3][1];
        let z = m[0][2] + m[3][2];
        assert!((x - 1.0).abs() < 1e-6 && y.abs() < 1e-6 && (z - 0.5).abs() < 1e-6);
        // yaw=90°：+x 旋到 +z（行0 = (0,0,1) 意味 ndc.x 取世界 z）。
        let m2 = orbit_mvp(std::f32::consts::FRAC_PI_2, 0.0, 1.0, 1.0);
        assert!(m2[0][0].abs() < 1e-6, "世界 x 不再贡献 ndc.x");
        assert!((m2[2][0] - 1.0).abs() < 1e-6, "世界 z 贡献 ndc.x");
    }
}
