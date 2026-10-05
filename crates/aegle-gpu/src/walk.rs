//! Incremental scene walk: geometry is recorded here, other commands go back.
use aegle_scene::{Affine, Command, MAX_SCOPE_DEPTH, RoundedRect, Scene};
use aegle_types::Rect;

use crate::{
    Error, Recording, Result,
    records::{NO_CLIP, State},
};

/// Outcome of one [`Walker::step`].
pub enum Step {
    /// The scene is exhausted.
    Done,
    /// A scope or geometry command was consumed.
    Recorded,
    /// A command the backend must handle (glyphs, images, paths, ...), with the
    /// state it draws under. Backends reject commands they do not implement.
    Command(Command, State),
}

/// Walks one scene under a transform and optional device-space clip. Stepping one
/// command at a time lets the backend flush its records between commands.
pub struct Walker<'a> {
    commands: std::slice::Iter<'a, Command>,
    state: State,
    saved: [State; MAX_SCOPE_DEPTH],
    depth: usize,
    viewport: [f32; 2],
}

impl<'a> Walker<'a> {
    /// Validates the clip depth and opens the external clip, if any. `viewport`
    /// comes from [`crate::viewport`].
    pub fn new(
        scene: &'a Scene,
        transform: Affine,
        clip: Option<Rect>,
        extent: [u32; 2],
        viewport: [f32; 2],
        recording: &mut Recording,
    ) -> Result<Self> {
        if scene.max_clip_depth() + usize::from(clip.is_some()) > 8 {
            return Err(Error::ClipDepth);
        }
        let mut state = State {
            transform,
            clip: NO_CLIP,
            bounds: [0.0, 0.0, extent[0] as f32, extent[1] as f32],
        };
        if let Some(rect) = clip {
            let shape = RoundedRect::new(rect, 0.0)?;
            recording.push_clip(&mut state, shape, Affine::IDENTITY)?;
        }
        Ok(Self {
            commands: scene.commands().iter(),
            state,
            // Scene scopes have a validated maximum: no per-walk heap scratch.
            saved: [state; MAX_SCOPE_DEPTH],
            depth: 0,
            viewport,
        })
    }

    /// Consumes the next command.
    pub fn step(&mut self, recording: &mut Recording) -> Result<Step> {
        let Some(command) = self.commands.next() else {
            return Ok(Step::Done);
        };
        let state = self.state;
        match *command {
            Command::PushTransform(local) => {
                self.saved[self.depth] = state;
                self.depth += 1;
                self.state.transform = local.then(state.transform)?;
            }
            Command::PushClip(shape) => {
                self.saved[self.depth] = state;
                self.depth += 1;
                recording.push_clip(&mut self.state, shape, state.transform)?;
            }
            Command::Pop => {
                self.depth -= 1;
                self.state = self.saved[self.depth];
            }
            Command::Fill { shape, color } => {
                recording.shape(state, shape, color, -1.0, self.viewport)?;
            }
            Command::Stroke {
                shape,
                color,
                width,
            } => {
                recording.shape(state, shape, color, width, self.viewport)?;
            }
            #[allow(unreachable_patterns)]
            command => return Ok(Step::Command(command, state)),
        }
        Ok(Step::Recorded)
    }
}
