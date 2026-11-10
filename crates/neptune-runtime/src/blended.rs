//! The blended pass: see-through surfaces and particles, drawn after
//! everything solid.
//!
//! Fyrox draws its whole forward stage in one list, ordered by the distance
//! of each mesh's origin. That order says nothing about whether a surface is
//! solid: a large opaque mesh (a landscape, say) whose origin is behind the
//! camera is drawn after a see-through surface right in front of it, and
//! paints over it or is rejected by its depth.
//!
//! So the shaders of see-through surfaces name their pass [`PASS`] instead
//! of `Forward`, and this render pass draws them once the forward stage has
//! finished. Fyrox's list is already sorted back to front, which is the order
//! blending needs.

use std::any::TypeId;

use fyrox::{
    core::sstorage::ImmutableString,
    graphics::error::FrameworkError,
    renderer::{bundle::BundleRenderContext, bundle::RenderDataBundle, RenderPassStatistics, SceneRenderPass, SceneRenderPassContext},
    scene::mesh::RenderPath,
};

/// The name of the shader pass drawn here.
pub const PASS: &str = "NeptuneBlended";

/// Draws every surface whose shader has the blended pass.
pub struct BlendedPass {
    plugin: TypeId,
    name: ImmutableString,
}

impl BlendedPass {
    pub fn new(plugin: TypeId) -> Self {
        BlendedPass { plugin, name: ImmutableString::new(PASS) }
    }
}

/// Only shaders with the pass are drawn, because Fyrox reports an error for
/// any other material asked to draw a pass it does not have.
fn has_blended_pass(bundle: &RenderDataBundle) -> bool {
    bundle.render_path == RenderPath::Forward
        && bundle.material.data_ref().as_loaded_ref().is_some_and(|m| {
            m.shader().data_ref().as_loaded_ref().is_some_and(|s| s.definition.passes.iter().any(|p| p.name == PASS))
        })
}

impl SceneRenderPass for BlendedPass {
    fn on_hdr_render(&mut self, ctx: SceneRenderPassContext) -> Result<RenderPassStatistics, FrameworkError> {
        ctx.bundle_storage.render_to_frame_buffer(
            ctx.server,
            ctx.geometry_cache,
            ctx.shader_cache,
            has_blended_pass,
            |_| true,
            BundleRenderContext {
                texture_cache: ctx.texture_cache,
                render_pass_name: &self.name,
                frame_buffer: ctx.framebuffer,
                viewport: ctx.observer.viewport,
                uniform_memory_allocator: ctx.uniform_memory_allocator,
                resource_manager: ctx.resource_manager,
                use_pom: ctx.quality_settings.use_parallax_mapping,
                light_position: &Default::default(),
                renderer_resources: ctx.renderer_resources,
                ambient_light: ctx.scene.rendering_options.ambient_lighting_color,
                scene_depth: Some(ctx.depth_texture),
            },
        )
    }

    fn source_type_id(&self) -> TypeId {
        self.plugin
    }
}
