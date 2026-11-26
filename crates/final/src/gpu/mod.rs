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
//! Why sample-major rather than strip-major (Y6): the accumulators of every band stay on the device
//! for the whole run, so one sample over the whole image refines the picture already on screen instead
//! of finishing a strip nobody can see. A checkpoint averages the accumulators down to a display-sized
//! picture on the device (`gpu/reduce.wgsl`) and only that picture crosses the bus, because a plate's
//! accumulator is half a gigabyte.
//!
//! Refuses: a device that cannot compile the shader or allocate a buffer is a [`FinalError::Device`]
//! with the driver's own sentence; a size or a frame past the crate's caps is the cap's own error.
//!
//! Changes when: the shader's interface, the accumulator's layout or the cache's key changes.

mod layout;

pub(crate) use layout::Uniform;

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use wgpu::util::DeviceExt as _;

use scene::{Camera, Lighting, SceneFrame};

use crate::image::Image;
use crate::soup::faces_of;
use crate::{Bvh, FinalError, Request};

/// The most pixels one accumulator buffer holds.
///
/// A device's largest storage binding is 128 MB by default and the accumulator is sixty-four bytes per
/// pixel — its sums, its second moment and the guide the filter reads — so a million pixels is a band
/// that fits on any device the editor runs on. The image is traced in bands of whole rows, which keeps
/// the cap on the operator's paper rather than the binding.
const MAX_STRIP_PIXELS: u64 = 1_000_000;

/// The longest side of a snapshot, in pixels.
///
/// A picture the shell shows is a display picture: a thousand pixels is more than the place's room
/// holds, and a bigger one would cost a quarter of a gigabyte a second across the bus for pixels
/// nobody can see. The finished image is unaffected — the file is written from the accumulators.
const SNAPSHOT_LONG_SIDE: u32 = 1024;

/// The shortest time between two snapshots of one run.
///
/// Four pictures a second is what a person reads as "it is moving" and what keeps the reduce pass, the
/// read-back and the tone map off the critical path of a sample: at a print plate that is one
/// checkpoint every couple of samples, and it costs a fraction of the sample it interrupts.
const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(250);

/// How many snapshots a run publishes however short it is.
///
/// An interval alone would publish nothing for a draft that takes 60 ms, and a page with no picture is
/// the defect this task exists to close. So a run also publishes at evenly spaced sample counts, and
/// the two rules together cover a second-long draft and a minute-long plate.
const MIN_CHECKPOINTS: u32 = 8;

/// How many rows beyond the ones a band owns the filter reads, so its widest tap has a neighbour.
///
/// The à-trous kernel is five taps wide and its last level steps sixteen pixels, so a pixel at the
/// band's edge reaches thirty-two rows past it. Those rows are not traced twice — the accumulators of
/// the whole image are held for the run — they are read out of the neighbouring band's buffer.
fn pad_rows() -> u32 {
    2 << (crate::denoise::LEVELS.saturating_sub(1))
}

/// How a run's snapshots are sized: the block one snapshot pixel averages and the picture it makes.
///
/// The block is a power of two, so the bands of the run — which own whole blocks each — cut the image
/// on the snapshot's own grid and a snapshot pixel is never the average of part of two bands. One
/// block is a snapshot pixel for pixel when the image already fits [`SNAPSHOT_LONG_SIDE`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SnapshotShape {
    /// How many image pixels a snapshot pixel averages, on both axes.
    stride: u32,
    /// The snapshot's width in pixels.
    width: u32,
    /// The snapshot's height in pixels.
    height: u32,
}

impl SnapshotShape {
    /// The shape a snapshot of a `width` by `height` image has.
    fn of(width: u32, height: u32) -> Self {
        let longest = width.max(height).max(1);
        let mut stride = 1u32;
        while longest.div_ceil(stride) > SNAPSHOT_LONG_SIDE {
            stride = stride.saturating_mul(2);
        }
        Self { stride, width: width.max(1).div_ceil(stride), height: height.max(1).div_ceil(stride) }
    }

    /// How many pixels the picture has.
    fn pixels(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}

/// The image rows one band of the accumulator owns, with the buffers and bind groups behind them.
struct Band {
    /// The first image row of the band.
    first_row: u32,
    /// How many rows it owns: whole snapshot blocks.
    rows: u32,
    /// The accumulator the integrator adds into, one [`layout::GpuPixel`] per pixel of the band.
    accumulator: wgpu::Buffer,
    /// Group one of the integrator: the accumulator it adds into.
    tracing: wgpu::BindGroup,
    /// Group zero of the reduce pass: that buffer read, and the block it is averaged by.
    reading: wgpu::BindGroup,
}

/// The accumulators of one run: every band of the image, and the picture its checkpoints are made of.
struct Accumulators {
    /// The bands, in image order; they partition every row of the image exactly once.
    bands: Vec<Band>,
    /// One `vec4` per snapshot pixel: the block's summed radiance and its summed sample count.
    picture: wgpu::Buffer,
    /// Group one of the reduce pass: the picture every band writes its own rows of.
    picture_bind: wgpu::BindGroup,
    /// The picture's size and the block it averages.
    snapshot: SnapshotShape,
}

/// The tracer's state on the device: its two pipelines and the frame it last uploaded.
pub struct Tracer {
    pipeline: wgpu::ComputePipeline,
    reduce: wgpu::ComputePipeline,
    scene_layout: wgpu::BindGroupLayout,
    image_layout: wgpu::BindGroupLayout,
    reduce_source_layout: wgpu::BindGroupLayout,
    reduce_output_layout: wgpu::BindGroupLayout,
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
            entries: &[
                // One pixel's sums, its second moment and its guide: one buffer, one binding.
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: storage(false),
                    count: None,
                },
            ],
        });
        // The reduce pass reads one band's accumulator and writes its rows of the snapshot picture:
        // two groups, because the source changes with the band and the picture does not.
        let reduce_source_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Z3L final render reduce source layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: storage(true),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<layout::ReduceUniform>() as u64),
                    },
                    count: None,
                },
            ],
        });
        let reduce_output_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Z3L final render reduce picture layout"),
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
        // The checkpoint's pass is its own module: one entry point that averages a band down to the
        // picture the shell shows, and no scene at all.
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let reduce_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Z3L final render reduce"),
            source: wgpu::ShaderSource::Wgsl(include_str!("reduce.wgsl").into()),
        });
        let reduce_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Z3L final render reduce pipeline layout"),
            bind_group_layouts: &[Some(&reduce_source_layout), Some(&reduce_output_layout)],
            immediate_size: 0,
        });
        let reduce = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Z3L final render reduce pipeline"),
            layout: Some(&reduce_layout),
            module: &reduce_shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(FinalError::Device { reason: format!("the checkpoint's pass does not compile: {error}") });
        }
        Ok(Self {
            pipeline,
            reduce,
            scene_layout,
            image_layout,
            reduce_source_layout,
            reduce_output_layout,
            uploaded: None,
            builds: 0,
        })
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

    /// Traces one image of the uploaded frame, one sample over the whole image at a time.
    ///
    /// The accumulators of every band live for the whole run, so a sample refines the picture a
    /// person is already looking at instead of finishing a strip nobody can see. Each sample is
    /// dispatched band by band with a submit and a fence between, so a cancel and the progress bar
    /// have somewhere to happen; a checkpoint averages the accumulators down to a display-sized
    /// picture on the device and publishes it into `snapshots`, and the image itself is filtered once
    /// at the end — five à-trous levels over the guide of each band — so what comes back is the
    /// picture and, beside it, the raw sums a reference can be measured from.
    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera: Camera,
        lighting: &Lighting,
        request: Request,
        watch: Watch<'_>,
    ) -> Result<Accumulation, FinalError> {
        let Some(uploaded) = self.uploaded.as_ref() else {
            return Err(FinalError::Device {
                reason: "no frame was uploaded: a final render prepares the frame before it traces".to_owned(),
            });
        };
        let Ok(capacity) = usize::try_from(request.pixels()) else {
            return Err(FinalError::ImageSize { width: request.width(), height: request.height() });
        };
        let accumulators = self.accumulate(device, request)?;
        self.trace(device, queue, uploaded, &accumulators, camera, lighting, request, watch)?;
        let mut sums = vec![[0.0f32; 4]; capacity];
        let mut denoised = vec![[0.0f32; 3]; capacity];
        for band in &accumulators.bands {
            // The rows this band owns, and the pad the filter's widest tap reaches into. The pad rows
            // are read from the band beside it: they were traced once, for that band.
            let start = band.first_row.saturating_sub(pad_rows());
            let end = band.first_row.saturating_add(band.rows).saturating_add(pad_rows()).min(request.height());
            let rows = end.saturating_sub(start);
            let traced: Vec<layout::GpuPixel> =
                read_rows(device, queue, &accumulators.bands, request.width(), start, rows)?;
            let picture = denoise_strip(request.width(), rows, &traced)?;
            let width = u64::from(request.width());
            let Ok(from) = usize::try_from(u64::from(band.first_row - start) * width) else { continue };
            let Ok(length) = usize::try_from(u64::from(band.rows) * width) else { continue };
            let Ok(into) = usize::try_from(u64::from(band.first_row) * width) else { continue };
            let to = from.saturating_add(length);
            if let (Some(target), Some(guide), Some(raw)) =
                (denoised.get_mut(into..into.saturating_add(length)), picture.get(from..to), traced.get(from..to))
            {
                target.copy_from_slice(guide);
                if let Some(sums) = sums.get_mut(into..into.saturating_add(length)) {
                    for (sum, pixel) in sums.iter_mut().zip(raw) {
                        *sum = pixel.sum;
                    }
                }
            }
        }
        Ok(Accumulation {
            width: request.width(),
            height: request.height(),
            samples: request.samples(),
            sums,
            denoised,
        })
    }

    /// Allocates one accumulator per band of the image and the picture its checkpoints are made of.
    ///
    /// Bands own whole blocks of the snapshot, so a snapshot pixel is averaged inside one band and
    /// never across two, and the reduce pass can run band by band into the same picture buffer. The
    /// accumulators are never re-created during a run: a second allocation would throw away the
    /// samples a checkpoint is made of.
    fn accumulate(&self, device: &wgpu::Device, request: Request) -> Result<Accumulators, FinalError> {
        let snapshot = SnapshotShape::of(request.width(), request.height());
        let width = request.width().max(1);
        // A band owns whole snapshot blocks, so the reduce pass can average it into the picture
        // without a block being split across two bands.
        let block_rows = MAX_STRIP_PIXELS / u64::from(width);
        let rows_per_band = block_rows.div_ceil(u64::from(snapshot.stride)) * u64::from(snapshot.stride);
        let rows_per_band = rows_per_band.max(u64::from(snapshot.stride)).min(u64::from(u32::MAX)) as u32;
        let mut bands = Vec::new();
        let mut first_row = 0u32;
        while first_row < request.height() {
            let rows = rows_per_band.min(request.height() - first_row);
            let pixels = u64::from(width) * u64::from(rows);
            let accumulator = strip_buffer(device, pixels);
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let tracing = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Z3L final render accumulator"),
                layout: &self.image_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: accumulator.as_entire_binding() }],
            });
            let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Z3L final render reduce uniform"),
                contents: bytemuck::bytes_of(&layout::ReduceUniform {
                    band: [width, rows, snapshot.stride, first_row],
                    picture: [snapshot.width, snapshot.height, 0, 0],
                }),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let reading = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Z3L final render reduce source"),
                layout: &self.reduce_source_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: accumulator.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: uniform.as_entire_binding() },
                ],
            });
            if let Some(error) = pollster::block_on(scope.pop()) {
                return Err(FinalError::Device { reason: format!("the accumulator does not fit the device: {error}") });
            }
            bands.push(Band { first_row, rows, accumulator, tracing, reading });
            first_row = first_row.saturating_add(rows.max(1));
        }
        let picture = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Z3L final render snapshot"),
            size: std::mem::size_of::<[f32; 4]>() as u64 * snapshot.pixels().max(1),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let picture_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Z3L final render reduce picture"),
            layout: &self.reduce_output_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: picture.as_entire_binding() }],
        });
        Ok(Accumulators { bands, picture, picture_bind, snapshot })
    }

    /// One sample over every band, until the request is satisfied or the person cancels.
    ///
    /// A cancel is honoured at a band boundary, so a stopped run has already published the picture of
    /// the samples it did take and leaves the destination untouched: nothing here writes a file.
    #[allow(clippy::too_many_arguments)]
    fn trace(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        uploaded: &Uploaded,
        accumulators: &Accumulators,
        camera: Camera,
        lighting: &Lighting,
        request: Request,
        watch: Watch<'_>,
    ) -> Result<(), FinalError> {
        let counts = (uploaded.face_count, uploaded.node_count);
        let mut checkpoints = Checkpoints::new(request.samples());
        for sample in 0..request.samples() {
            for band in &accumulators.bands {
                if watch.progress.cancelled() {
                    return Err(watch.progress.refusal());
                }
                let uniform = layout::uniform(camera, lighting, request, counts, (band.first_row, band.rows), sample)?;
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
                    pass.set_bind_group(1, &band.tracing, &[]);
                    let workgroups = |extent: u32| extent.div_ceil(8).max(1);
                    pass.dispatch_workgroups(workgroups(request.width()), workgroups(band.rows), 1);
                }
                queue.submit([encoder.finish()]);
                device.poll(wgpu::PollType::wait_indefinitely()).map_err(|error| FinalError::Device {
                    reason: format!("the device did not finish a sample: {error}"),
                })?;
            }
            watch.progress.advance();
            if checkpoints.due(watch.progress.done(), watch.progress.total()) {
                self.publish(device, queue, accumulators, watch.progress.done(), watch.snapshots)?;
            }
        }
        Ok(())
    }

    /// Averages the accumulators down to a display-sized picture and hands it over, or drops it when
    /// nobody has taken the previous one.
    ///
    /// The averaging is the device's (`gpu/reduce.wgsl`): only the snapshot buffer — a megapixel at
    /// worst — is read back, and the tone map that turns it into pixels is the shell's, so the
    /// exposure can be dragged over the last picture without a second render.
    fn publish(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        accumulators: &Accumulators,
        samples: u32,
        snapshots: &Snapshots,
    ) -> Result<(), FinalError> {
        let snapshot = accumulators.snapshot;
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Z3L final render snapshot") });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Z3L final render snapshot"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.reduce);
            pass.set_bind_group(1, &accumulators.picture_bind, &[]);
            let workgroups = |extent: u32| extent.div_ceil(8).max(1);
            for band in &accumulators.bands {
                pass.set_bind_group(0, &band.reading, &[]);
                let rows = band.rows.div_ceil(snapshot.stride).max(1);
                pass.dispatch_workgroups(workgroups(snapshot.width), workgroups(rows), 1);
            }
        }
        queue.submit([encoder.finish()]);
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| FinalError::Device { reason: format!("the device did not finish a snapshot: {error}") })?;
        let Ok(pixels) = usize::try_from(snapshot.pixels()) else {
            return Err(FinalError::ImageSize { width: snapshot.width, height: snapshot.height });
        };
        let sums: Vec<[f32; 4]> = read_back(device, queue, &accumulators.picture, 0, pixels)?;
        let image = Image::from_accumulation(snapshot.width, snapshot.height, &sums, samples.max(1))?;
        snapshots.publish(Snapshot { image, samples });
        Ok(())
    }
}

/// When a run publishes a picture: often enough to look alive, seldom enough to pay for nothing.
///
/// Two rules, because one is not enough: a plate publishes every [`SNAPSHOT_INTERVAL`] whatever its
/// sample count, and a draft that finishes in the interval still publishes [`MIN_CHECKPOINTS`] evenly
/// spaced pictures instead of none. Both are counted in samples and time the loop already has, so the
/// decision costs a subtraction.
struct Checkpoints {
    /// The sample interval that publishes however short the run is.
    every: u32,
    /// When the last picture was published.
    last: Instant,
}

impl Checkpoints {
    /// The checkpoints of a run of `samples` samples.
    fn new(samples: u32) -> Self {
        Self { every: samples.div_ceil(MIN_CHECKPOINTS).max(1), last: Instant::now() }
    }

    /// Whether the picture after `done` samples is worth publishing.
    ///
    /// Never the last one: the finished image follows it, and two pictures of the same sample count
    /// would only teach a viewer that the number does not move.
    fn due(&mut self, done: u32, total: u32) -> bool {
        if done >= total {
            return false;
        }
        if !done.is_multiple_of(self.every) && self.last.elapsed() < SNAPSHOT_INTERVAL {
            return false;
        }
        self.last = Instant::now();
        true
    }
}

/// What a surface watches while a run works: the counter it reads and the pictures it takes.
///
/// The two travel together because a run answers a surface in exactly two ways — how far it is, and
/// what it looks like — and because neither is a return value: a picture arrives many times during
/// one call, and the answer the call itself gives is the file.
#[derive(Clone, Copy, Debug)]
pub struct Watch<'a> {
    /// The run's cancel flag and its sample counter.
    pub progress: &'a Progress,
    /// The mailbox the run publishes its pictures into.
    pub snapshots: &'a Snapshots,
}

/// One picture of a run at the size a screen shows it: linear radiance per pixel, and the samples
/// behind it.
///
/// It carries radiance and not bytes on purpose: the file's curve is the shell's, so dragging the
/// exposure re-tones the picture it already has instead of paying for another run (Y6).
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    image: Image,
    samples: u32,
}

impl Snapshot {
    /// `image` as a snapshot of `samples`, averaged down to a display size when the image is larger.
    ///
    /// A block average is what a smaller picture of the same light is: the mean of the pixels, not the
    /// mean of the means, so a bright window in a block still counts as its area.
    pub fn of(image: &Image, samples: u32) -> Result<Self, FinalError> {
        let shape = SnapshotShape::of(image.width(), image.height());
        if shape.stride == 1 && shape.width == image.width() && shape.height == image.height() {
            return Ok(Self { image: image.clone(), samples });
        }
        let mut radiance = Vec::with_capacity(usize::try_from(shape.pixels()).unwrap_or(0));
        for row in 0..shape.height {
            let top = row * shape.stride;
            for column in 0..shape.width {
                let left = column * shape.stride;
                let mut sum = [0.0f64; 3];
                let mut count = 0.0f64;
                for y in top..top.saturating_add(shape.stride).min(image.height()) {
                    for x in left..left.saturating_add(shape.stride).min(image.width()) {
                        if let Some(pixel) = image.radiance_at(x, y) {
                            sum[0] += f64::from(pixel[0]);
                            sum[1] += f64::from(pixel[1]);
                            sum[2] += f64::from(pixel[2]);
                            count += 1.0;
                        }
                    }
                }
                let count = if count > 0.0 { count } else { 1.0 };
                radiance.push([(sum[0] / count) as f32, (sum[1] / count) as f32, (sum[2] / count) as f32]);
            }
        }
        Ok(Self { image: Image::from_linear(shape.width, shape.height, &radiance)?, samples })
    }

    /// The picture itself, as linear radiance the shell tone-maps.
    pub fn image(&self) -> &Image {
        &self.image
    }

    /// The picture's width in pixels.
    pub fn width(&self) -> u32 {
        self.image.width()
    }

    /// The picture's height in pixels.
    pub fn height(&self) -> u32 {
        self.image.height()
    }

    /// How many samples the picture is made of.
    pub fn samples(&self) -> u32 {
        self.samples
    }
}

/// The one slot a run publishes its pictures into.
///
/// Bounded, not a queue: a picture nobody has taken is replaced by the next one, so a window that is
/// busy, hidden or slow loses pictures instead of growing memory behind a thread that renders faster
/// than the screen refreshes. The counters say what happened — how many were published and how many
/// were dropped unread — so the behaviour is a test and not a hope.
#[derive(Debug, Default)]
pub struct Snapshots {
    slot: Mutex<Option<Snapshot>>,
    published: AtomicU64,
    replaced: AtomicU64,
}

impl Snapshots {
    /// A mailbox nobody has published into.
    pub fn new() -> Self {
        Self::default()
    }

    /// Publishes `snapshot`, replacing one nobody has taken.
    pub fn publish(&self, snapshot: Snapshot) {
        let mut slot = self.lock();
        if slot.replace(snapshot).is_some() {
            self.replaced.fetch_add(1, Ordering::Relaxed);
        }
        drop(slot);
        self.published.fetch_add(1, Ordering::Relaxed);
    }

    /// Takes the picture waiting, if there is one.
    pub fn take(&self) -> Option<Snapshot> {
        self.lock().take()
    }

    /// How many pictures were published, taken or not.
    pub fn published(&self) -> u64 {
        self.published.load(Ordering::Relaxed)
    }

    /// How many pictures were published while an unread one waited, and so never seen.
    pub fn replaced(&self) -> u64 {
        self.replaced.load(Ordering::Relaxed)
    }

    /// The slot, even when a thread that died inside a publish poisoned the lock: a lost picture is
    /// not a reason to lose the next one, and `AGENTS.md` §4.4 asks for a fallback, not a panic.
    fn lock(&self) -> MutexGuard<'_, Option<Snapshot>> {
        match self.slot.lock() {
            Ok(slot) => slot,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// One band's buffer: a [`layout::GpuPixel`] for every pixel it traces.
///
/// One buffer rather than three, because a compute shader has eight storage bindings in a stage and
/// the frame's own arrays take six of them; and one allocation, because a sample writes all four
/// `vec4`s of a pixel anyway. The device zeroes it: an accumulator that began at a previous run's
/// numbers would average two pictures together.
fn strip_buffer(device: &wgpu::Device, pixels: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Z3L final render band"),
        size: (std::mem::size_of::<layout::GpuPixel>() as u64).saturating_mul(pixels).max(16),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

/// The picture one band's pixels make: their means, their variance, and five à-trous levels.
///
/// The averaging and the filter are the host's (`crate::denoise`), which is where the tests measure
/// them; this function is the one place a band's rows become that frame.
fn denoise_strip(width: u32, height: u32, traced: &[layout::GpuPixel]) -> Result<Vec<[f32; 3]>, FinalError> {
    let pixels: Vec<crate::denoise::Sample> = traced.iter().map(layout::GpuPixel::sample).collect();
    let guides: Vec<crate::denoise::Guide> = traced.iter().map(layout::GpuPixel::guide).collect();
    let frame = crate::denoise::Frame::new(width, height, pixels, guides)?;
    Ok(frame.denoise())
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

/// Copies `rows` image rows starting at `first_row` out of the bands that hold them.
///
/// A band's filter reaches [`pad_rows`] rows past its own, and those rows belong to the neighbour: the
/// copy names each band's own overlap of the range, so what the filter reads is the image's rows and
/// not one band's. Nothing is traced twice for it — every row of the image has one accumulator.
fn read_rows<T: bytemuck::Pod>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bands: &[Band],
    width: u32,
    first_row: u32,
    rows: u32,
) -> Result<Vec<T>, FinalError> {
    let bytes_per_row = u64::from(width).saturating_mul(std::mem::size_of::<T>() as u64);
    let Ok(pixels) = usize::try_from(u64::from(width).saturating_mul(u64::from(rows))) else {
        return Err(FinalError::ImageSize { width, height: rows });
    };
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Z3L final render read-back"),
        size: bytes_per_row.saturating_mul(u64::from(rows)).max(16),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Z3L final render read-back") });
    let end = first_row.saturating_add(rows);
    let mut copied = false;
    for band in bands {
        let start = band.first_row.max(first_row);
        let stop = band.first_row.saturating_add(band.rows).min(end);
        if stop <= start {
            continue;
        }
        let source = u64::from(start - band.first_row).saturating_mul(bytes_per_row);
        let destination = u64::from(start - first_row).saturating_mul(bytes_per_row);
        let size = u64::from(stop - start).saturating_mul(bytes_per_row);
        encoder.copy_buffer_to_buffer(&band.accumulator, source, &staging, destination, size);
        copied = true;
    }
    if copied {
        queue.submit([encoder.finish()]);
    }
    map_back::<T>(device, staging, pixels)
}

/// Copies one buffer's first `pixels` elements back to the host.
fn read_back<T: bytemuck::Pod>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    source: &wgpu::Buffer,
    offset: u64,
    pixels: usize,
) -> Result<Vec<T>, FinalError> {
    let bytes = (std::mem::size_of::<T>() as u64).saturating_mul(pixels as u64).max(16);
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Z3L final render read-back"),
        size: bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("Z3L final render read-back") });
    encoder.copy_buffer_to_buffer(source, offset, &staging, 0, bytes);
    queue.submit([encoder.finish()]);
    map_back::<T>(device, staging, pixels)
}

/// Maps a staged buffer and reads `pixels` elements of `T` out of it.
fn map_back<T: bytemuck::Pod>(
    device: &wgpu::Device,
    staging: wgpu::Buffer,
    pixels: usize,
) -> Result<Vec<T>, FinalError> {
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
    let mut values = Vec::with_capacity(pixels);
    {
        let mapped = slice
            .get_mapped_range()
            .map_err(|error| FinalError::Device { reason: format!("the image is not mapped: {error}") })?;
        let read: &[T] = bytemuck::try_cast_slice(&mapped)
            .map_err(|error| FinalError::Device { reason: format!("the accumulator is not readable: {error}") })?;
        values.extend_from_slice(&read[..read.len().min(pixels)]);
    }
    staging.unmap();
    Ok(values)
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

/// The linear radiance of one image: what the tracer summed, and what the filter made of it.
///
/// Both pictures travel because they answer two questions: the sums are the raw estimator, which a
/// reference is measured against and a test reads a single sample's value from, and `denoised` is the
/// picture the file is written of.
#[derive(Clone, Debug, PartialEq)]
pub struct Accumulation {
    width: u32,
    height: u32,
    samples: u32,
    sums: Vec<[f32; 4]>,
    denoised: Vec<[f32; 3]>,
}

impl Accumulation {
    /// The image the filter made, which is the picture a render writes.
    pub fn into_image(self) -> Result<Image, FinalError> {
        Image::from_linear(self.width, self.height, &self.denoised)
    }

    /// The image as the samples alone average to, before any filter touched it.
    ///
    /// A test comparing a render against a reference uses this one: the filter is what is being
    /// measured, so it must not be able to hide inside the reference.
    pub fn raw_image(&self) -> Result<Image, FinalError> {
        Image::from_accumulation(self.width, self.height, &self.sums, self.samples)
    }

    /// The raw sums, per pixel: three channels of radiance and the number of samples behind them.
    pub fn sums(&self) -> &[[f32; 4]] {
        &self.sums
    }

    /// The denoised radiance, per pixel, row-major from the top.
    pub fn denoised(&self) -> &[[f32; 3]] {
        &self.denoised
    }

    /// How many samples each pixel was offered, which is the most any of them took.
    pub fn samples(&self) -> u32 {
        self.samples
    }

    /// How many samples one pixel actually took, which is fewer than [`Accumulation::samples`] where
    /// the pixel converged and retired (`crate::denoise::MIN_SAMPLES`).
    pub fn sample_count(&self, x: u32, y: u32) -> Option<u32> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let at = usize::try_from(u64::from(y) * u64::from(self.width) + u64::from(x)).ok()?;
        self.sums.get(at).map(|sum| sum[3].max(0.0) as u32)
    }

    /// The image width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The image height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    /// One picture of a known colour, so a snapshot's own arithmetic is what is measured.
    fn picture(width: u32, height: u32, left: f32, right: f32) -> Image {
        let mut radiance = Vec::new();
        for _row in 0..height {
            for column in 0..width {
                let value = if column < width / 2 { left } else { right };
                radiance.push([value, value, value]);
            }
        }
        Image::from_linear(width, height, &radiance).expect("a picture of two values")
    }

    #[test]
    fn a_snapshot_is_the_whole_frame_averaged_down_to_a_display_size() {
        // An image wider than the snapshot's cap is averaged by blocks, never cropped or stretched.
        let wide = SnapshotShape::of(2048, 1024);
        assert_eq!((wide.stride, wide.width, wide.height), (2, 1024, 512), "{wide:?}");
        let image = picture(2048, 1024, 0.25, 0.75);
        let snapshot = Snapshot::of(&image, 4).expect("the picture is a snapshot");
        assert_eq!((snapshot.width(), snapshot.height(), snapshot.samples()), (1024, 512, 4));
        let middle = snapshot.image().radiance_at(256, 10).expect("a pixel of the middle of the left half");
        assert!((middle[0] - 0.25).abs() < 1e-6, "a block of one value averages to it: {middle:?}");
        let edge = snapshot.image().radiance_at(512, 10).expect("the first pixel of the right half");
        assert!((edge[0] - 0.75).abs() < 1e-6, "the halves stay where they were: {edge:?}");

        // An image inside the cap is its own snapshot, pixel for pixel and without a copy of a copy.
        let small = picture(64, 32, 0.5, 0.5);
        let same = Snapshot::of(&small, 16).expect("a small picture is a snapshot");
        assert_eq!((same.width(), same.height()), (64, 32));
        assert_eq!(same.image(), &small, "a picture that already fits travels as it is");
    }

    #[test]
    fn the_mailbox_keeps_one_unread_picture_and_counts_the_ones_it_lost() {
        // Bounded, not a queue: this is what keeps a render faster than the screen from growing
        // memory behind it, and the counter is how a test sees it without a wall clock.
        let mailbox = Snapshots::new();
        assert!(mailbox.take().is_none(), "an empty mailbox hands over nothing");
        for samples in 1..=3 {
            let image = picture(4, 4, 0.1 * samples as f32, 0.1);
            mailbox.publish(Snapshot::of(&image, samples).expect("a picture"));
        }
        assert_eq!(mailbox.published(), 3);
        assert_eq!(mailbox.replaced(), 2, "two pictures were published over one nobody had taken");
        let taken = mailbox.take().expect("the last picture is waiting");
        assert_eq!(taken.samples(), 3, "the newest picture replaces the old one");
        assert!(mailbox.take().is_none(), "taking it leaves the slot empty");
        assert_eq!(mailbox.replaced(), 2, "a taken picture is not a replaced one");
        assert_eq!(mailbox.published(), 3, "publishing counts them all");
    }

    #[test]
    fn a_run_publishes_the_frame_often_and_never_twice_at_one_sample_count() {
        // Sixteen samples publish at every second one, and never at the last: the finished image
        // follows it with the same count (Y6).
        let mut checkpoints = Checkpoints::new(16);
        let mut published = Vec::new();
        for done in 1..=16 {
            if checkpoints.due(done, 16) {
                published.push(done);
            }
        }
        assert_eq!(published, vec![2, 4, 6, 8, 10, 12, 14], "a draft publishes while it runs");
        // A run shorter than the interval still publishes a handful, from the sample count alone.
        let mut short = Checkpoints::new(4);
        let several: Vec<u32> = (1..=4).filter(|done| short.due(*done, 4)).collect();
        assert!(several.len() >= 3, "{several:?}");
        // One sample has nothing to publish: the finished picture is the whole run.
        let mut single = Checkpoints::new(1);
        assert!(!single.due(1, 1), "the last sample is not a checkpoint");
    }
}
