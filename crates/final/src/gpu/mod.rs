//! The tracer on the device: the WGSL integrator, its buffers and the read-back.
//!
//! The crate borrows a [`wgpu::Device`] the shell already made — it opens no instance, chooses no
//! adapter and knows no surface — and owns exactly three things: the compute pipeline, the frame
//! uploaded as faces and nodes, and an accumulator that is read back and averaged. A render is a
//! loop of one dispatch per sample, so the progress a person watches and the cancel they press are
//! the loop's own counters rather than a guess (`crate::job`).
//!
//! The frame is cached by revision: the BVH is the expensive part of a final render (tens of
//! milliseconds for a kitchen) and a second image of the same document must not rebuild it. The key
//! is a revision number the shell supplies — a document counter, never a wall clock — so the same
//! document twice builds one tree and a changed one builds a second (`AGENTS.md` §4.2).
//!
//! Why one sample per dispatch instead of a loop inside the shader: a submit is the point at which a
//! cancel is honoured and the progress bar moves, and a storage buffer is the only thing a shader can
//! accumulate into. The cost is one fence per sample, small against a sample that traces an image.
//!
//! Refuses: a device that cannot compile the shader or allocate a buffer is a [`FinalError::Device`]
//! with the driver's own sentence; a size or a frame past the crate's caps is the cap's own error.
//!
//! Changes when: the shader's interface, the accumulator's layout or the cache's key changes.

mod layout;

pub(crate) use layout::Uniform;

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use wgpu::util::DeviceExt as _;

use scene::{Camera, Lighting, SceneFrame};

use crate::image::Image;
use crate::soup::faces_of;
use crate::{Bvh, FinalError, Request};

/// The most pixels one dispatch writes into one accumulator buffer.
///
/// A device's largest storage binding is 128 MB by default and the accumulator is sixteen bytes per
/// pixel, so two million pixels is a strip that fits with room for the faces and the nodes beside it
/// on any device the editor runs on. The image is traced strip by strip, which keeps the cap on an
/// image the operator's paper rather than the binding.
const MAX_STRIP_PIXELS: u64 = 2_000_000;

/// How many rows of an image one dispatch traces: whole rows, so a strip is a rectangle.
fn strip_rows(request: Request) -> u32 {
    let width = u64::from(request.width()).max(1);
    (MAX_STRIP_PIXELS / width).max(1).min(u64::from(request.height()).max(1)) as u32
}

/// The tracer's state on the device: one pipeline and the frame it last uploaded.
pub struct Tracer {
    pipeline: wgpu::ComputePipeline,
    scene_layout: wgpu::BindGroupLayout,
    image_layout: wgpu::BindGroupLayout,
    uploaded: Option<Uploaded>,
    builds: usize,
}

/// The frame as it lives on the device, with the revision it belongs to.
struct Uploaded {
    revision: u64,
    face_count: u32,
    node_count: u32,
    /// The camera, the lights and the counts: one uniform, rewritten for every sample.
    uniform: wgpu::Buffer,
    /// Group zero: the uniform, the faces and the nodes.
    bind_group: wgpu::BindGroup,
    /// The bind group keeps these alive on the device; the handles are held so a reader sees the
    /// frame the tracer owns instead of inferring it from a reference count.
    _faces: wgpu::Buffer,
    _nodes: wgpu::Buffer,
    _sobol: wgpu::Buffer,
    _energy: wgpu::Buffer,
    _decor_texels: wgpu::Buffer,
    _decor_entries: wgpu::Buffer,
}

impl Tracer {
    /// Compiles the integrator for `device`.
    ///
    /// The pipeline is created once for the life of the tracer, which is what makes a second image of
    /// the same session cheap: only the frame and the accumulator differ between renders.
    pub fn new(device: &wgpu::Device) -> Result<Self, FinalError> {
        // A shader that does not compile is a sentence the shell shows, not a process that dies inside
        // the driver: the scope catches what creation would otherwise report as a panic.
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Z3L final render integrator"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let storage = |read_only: bool| wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Z3L final render scene layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Uniform>() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: storage(true),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: storage(true),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: storage(true),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: storage(true),
                    count: None,
                },
                // The decors: the pictures' texels and the table that finds each one.
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: storage(true),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: storage(true),
                    count: None,
                },
            ],
        });
        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Z3L final render accumulator layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: storage(false),
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Z3L final render pipeline layout"),
            bind_group_layouts: &[Some(&scene_layout), Some(&image_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Z3L final render pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(FinalError::Device { reason: format!("the integrator does not compile: {error}") });
        }
        Ok(Self { pipeline, scene_layout, image_layout, uploaded: None, builds: 0 })
    }

    /// How many times a frame was uploaded, which is how a test sees the revision cache.
    pub fn build_count(&self) -> usize {
        self.builds
    }

    /// The revision the uploaded frame belongs to, or `None` before the first upload.
    pub fn revision(&self) -> Option<u64> {
        self.uploaded.as_ref().map(|uploaded| uploaded.revision)
    }

    /// Uploads `frame` for `revision`, or keeps the tree when that revision is already there.
    ///
    /// Returns whether the frame was rebuilt. The revision is the document's own counter — the number
    /// a mesh cache would be keyed by — so two renders of an unchanged document share one tree and
    /// one pair of buffers, and a changed document never reuses a stale one.
    pub fn prepare(&mut self, device: &wgpu::Device, frame: &SceneFrame, revision: u64) -> Result<bool, FinalError> {
        if self.uploaded.as_ref().is_some_and(|uploaded| uploaded.revision == revision) {
            return Ok(false);
        }
        let soup = faces_of(frame)?;
        let decors = layout::decors(&soup.decors);
        let tree = Bvh::build(soup.faces);
        let faces = layout::faces(&tree)?;
        let nodes = layout::nodes(&tree)?;
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Z3L final render uniform"),
            size: std::mem::size_of::<Uniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let face_buffer = storage_buffer(device, "Z3L final render faces", &faces);
        let node_buffer = storage_buffer(device, "Z3L final render nodes", &nodes);
        // The two tables the shader transcribes rather than retypes: the sampler's direction numbers
        // and the conductors' measured energy completion.
        let sobol = layout::sobol_directions();
        let energy = layout::energy_values();
        let sobol_buffer = storage_buffer(device, "Z3L final render sobol", &sobol);
        let energy_buffer = storage_buffer(device, "Z3L final render energy", &energy);
        let decor_texels = storage_buffer(device, "Z3L final render decor texels", &decors.texels);
        let decor_entries = storage_buffer(device, "Z3L final render decor entries", &decors.entries);
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Z3L final render scene"),
            layout: &self.scene_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: face_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: node_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: sobol_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: energy_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: decor_texels.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 6, resource: decor_entries.as_entire_binding() },
            ],
        });
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(FinalError::Device { reason: format!("the frame does not fit the device: {error}") });
        }
        let Ok(face_count) = u32::try_from(tree.face_count()) else {
            return Err(FinalError::Device { reason: "the frame has more faces than the device can index".to_owned() });
        };
        let Ok(node_count) = u32::try_from(tree.node_count()) else {
            return Err(FinalError::Device { reason: "the tree has more nodes than the device can index".to_owned() });
        };
        self.uploaded = Some(Uploaded {
            revision,
            face_count,
            node_count,
            uniform,
            bind_group,
            _faces: face_buffer,
            _nodes: node_buffer,
            _sobol: sobol_buffer,
            _energy: energy_buffer,
            _decor_texels: decor_texels,
            _decor_entries: decor_entries,
        });
        self.builds = self.builds.saturating_add(1);
        Ok(true)
    }

    /// Traces one image of the uploaded frame, one sample per dispatch.
    ///
    /// The result is summed radiance per pixel: the averaging belongs to [`Accumulation::into_image`],
    /// because that is where the sample count is divided once instead of once per sample.
    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: Camera,
        lighting: &Lighting,
        request: Request,
        progress: &Progress,
    ) -> Result<Accumulation, FinalError> {
        let Some(uploaded) = self.uploaded.as_ref() else {
            return Err(FinalError::Device {
                reason: "no frame was uploaded: a final render prepares the frame before it traces".to_owned(),
            });
        };
        let pixels = request.pixels();
        let Ok(capacity) = usize::try_from(pixels) else {
            return Err(FinalError::ImageSize { width: request.width(), height: request.height() });
        };
        let mut sums = vec![[0.0f32; 4]; capacity];
        let rows = strip_rows(request);
        let mut first_row = 0u32;
        while first_row < request.height() {
            let strip_rows = rows.min(request.height() - first_row);
            let strip_pixels = u64::from(request.width()) * u64::from(strip_rows);
            let Ok(strip_len) = usize::try_from(strip_pixels) else {
                return Err(FinalError::ImageSize { width: request.width(), height: request.height() });
            };
            let accumulator = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Z3L final render accumulator"),
                size: (std::mem::size_of::<[f32; 4]>() as u64).saturating_mul(strip_pixels).max(16),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let image_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Z3L final render accumulator"),
                layout: &self.image_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: accumulator.as_entire_binding() }],
            });
            if let Some(error) = pollster::block_on(scope.pop()) {
                return Err(FinalError::Device { reason: format!("the accumulator does not fit the device: {error}") });
            }
            for sample in 0..request.samples() {
                if progress.cancelled() {
                    return Err(progress.refusal());
                }
                let counts = (uploaded.face_count, uploaded.node_count);
                let uniform = layout::uniform(camera, lighting, request, counts, (first_row, strip_rows), sample)?;
                queue.write_buffer(&uploaded.uniform, 0, bytemuck::bytes_of(&uniform));
                let mut encoder = device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Z3L final render sample") });
                {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("Z3L final render sample"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.pipeline);
                    pass.set_bind_group(0, &uploaded.bind_group, &[]);
                    pass.set_bind_group(1, &image_bind_group, &[]);
                    let workgroups = |extent: u32| extent.div_ceil(8).max(1);
                    pass.dispatch_workgroups(workgroups(request.width()), workgroups(strip_rows), 1);
                }
                queue.submit([encoder.finish()]);
                device.poll(wgpu::PollType::wait_indefinitely()).map_err(|error| FinalError::Device {
                    reason: format!("the device did not finish a sample: {error}"),
                })?;
                progress.advance();
            }
            let strip = read_back(device, queue, &accumulator, strip_len)?;
            let start = usize::try_from(u64::from(first_row) * u64::from(request.width())).unwrap_or(0);
            if let Some(target) = sums.get_mut(start..start.saturating_add(strip.len())) {
                target.copy_from_slice(&strip);
            }
            first_row = first_row.saturating_add(strip_rows);
        }
        Ok(Accumulation { width: request.width(), height: request.height(), samples: request.samples(), sums })
    }
}

impl std::fmt::Debug for Tracer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Tracer").field("revision", &self.revision()).field("builds", &self.builds).finish()
    }
}

/// Uploads `values` as one read-only storage buffer, with one spare element when there are none.
///
/// A zero-length buffer is not a legal binding, and an empty frame is a legal frame: one zeroed
/// element is uploaded instead and the shader's own counts keep it unreachable.
fn storage_buffer<T: bytemuck::Pod>(device: &wgpu::Device, label: &str, values: &[T]) -> wgpu::Buffer {
    let bytes = bytemuck::cast_slice(values);
    let contents = if bytes.is_empty() { vec![0u8; std::mem::size_of::<T>()] } else { bytes.to_vec() };
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: &contents,
        usage: wgpu::BufferUsages::STORAGE,
    })
}

/// Copies a strip's accumulator back to the host.
fn read_back(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    accumulator: &wgpu::Buffer,
    pixels: usize,
) -> Result<Vec<[f32; 4]>, FinalError> {
    let bytes = (std::mem::size_of::<[f32; 4]>() as u64).saturating_mul(pixels as u64).max(16);
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Z3L final render read-back"),
        size: bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Z3L final render read-back") });
    encoder.copy_buffer_to_buffer(accumulator, 0, &staging, 0, bytes);
    queue.submit([encoder.finish()]);
    let slice = staging.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|error| FinalError::Device { reason: format!("the image could not be read back: {error}") })?;
    receiver
        .recv()
        .map_err(|_| FinalError::Device { reason: "the read-back never answered".to_owned() })?
        .map_err(|error| FinalError::Device { reason: format!("the image could not be mapped: {error}") })?;
    let mut sums = Vec::with_capacity(pixels);
    {
        let mapped = slice
            .get_mapped_range()
            .map_err(|error| FinalError::Device { reason: format!("the image is not mapped: {error}") })?;
        let values: &[[f32; 4]] = bytemuck::try_cast_slice(&mapped)
            .map_err(|error| FinalError::Device { reason: format!("the accumulator is not readable: {error}") })?;
        sums.extend_from_slice(&values[..values.len().min(pixels)]);
    }
    staging.unmap();
    Ok(sums)
}

/// The run's cancel flag and its sample counter, which a surface reads while a render runs.
///
/// The values are atomics because a window reads them on its own thread: the surface polls
/// [`Progress::done`] between frames and sets [`Progress::cancel`] from a button, and neither waits
/// for the tracer to reach a checkpoint.
#[derive(Debug)]
pub struct Progress {
    total: u32,
    done: AtomicU32,
    cancelled: AtomicBool,
}

impl Progress {
    /// A fresh run of `total` samples.
    pub fn new(total: u32) -> Self {
        Self { total, done: AtomicU32::new(0), cancelled: AtomicBool::new(false) }
    }

    /// How many samples the run takes.
    pub fn total(&self) -> u32 {
        self.total
    }

    /// How many samples have been traced.
    pub fn done(&self) -> u32 {
        self.done.load(Ordering::Relaxed)
    }

    /// How far the run is, as `0.0..=1.0`, which a progress bar draws.
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        self.done().min(self.total) as f32 / self.total as f32
    }

    /// Whether the person asked the render to stop.
    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Asks the render to stop at the next sample boundary.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// One more sample is done.
    fn advance(&self) {
        let done = self.done().saturating_add(1).min(self.total);
        self.done.store(done, Ordering::Relaxed);
    }

    /// The refusal a cancelled run reports, in the samples it took and the samples it owed.
    fn refusal(&self) -> FinalError {
        FinalError::Cancelled { samples: self.done(), total: self.total }
    }
}

/// The linear radiance of one image, summed over its samples and not yet divided.
#[derive(Clone, Debug, PartialEq)]
pub struct Accumulation {
    width: u32,
    height: u32,
    samples: u32,
    sums: Vec<[f32; 4]>,
}

impl Accumulation {
    /// The image with its samples averaged, or a refusal when the buffer does not fit the size.
    pub fn into_image(self) -> Result<Image, FinalError> {
        Image::from_accumulation(self.width, self.height, &self.sums, self.samples)
    }

    /// The raw sums, per pixel, for a test that wants one sample's own value.
    pub fn sums(&self) -> &[[f32; 4]] {
        &self.sums
    }

    /// How many samples each pixel took, which is what the sums are divided by.
    pub fn samples(&self) -> u32 {
        self.samples
    }
}
