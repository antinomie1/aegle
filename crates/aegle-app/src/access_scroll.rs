use crate::{
    Result,
    state::{Content, State},
};
use aegle_access::accesskit::{Action, ActionData, ScrollUnit};
use aegle_core::NodeId;
use aegle_types::Point;

impl State {
    pub(crate) fn has_scroll_ancestor(&self, id: NodeId) -> bool {
        let mut parent = self.tree.parent(id).unwrap();
        while let Some(id) = parent {
            if matches!(self.tree.get(id).unwrap().context.content, Content::Scroll) {
                return true;
            }
            parent = self.tree.parent(id).unwrap();
        }
        false
    }

    // Called after refresh and the shared enabled/visible check, before focus
    // eligibility: scroll containers and labels need not accept keyboard focus.
    pub(crate) fn access_scroll(
        &mut self,
        target: NodeId,
        action: Action,
        data: Option<&ActionData>,
    ) -> Result<Option<bool>> {
        if action == Action::ScrollIntoView {
            if data.is_some() || !self.has_scroll_ancestor(target) {
                return Ok(Some(false));
            }
            self.reveal(target)?;
            return Ok(Some(true));
        }
        if !matches!(
            action,
            Action::SetScrollOffset
                | Action::ScrollUp
                | Action::ScrollDown
                | Action::ScrollLeft
                | Action::ScrollRight
        ) {
            return Ok(None);
        }
        let element = &self.tree.get(target).unwrap().context;
        if !matches!(element.content, Content::Scroll) {
            return Ok(Some(false));
        }
        let limit = self.scroll_limit(target);
        let position = match (action, data) {
            (Action::SetScrollOffset, Some(ActionData::SetScrollOffset(point)))
                if point.x.is_finite() && point.y.is_finite() =>
            {
                Point::new(
                    point.x.clamp(0.0, f64::from(limit.x)) as f32,
                    point.y.clamp(0.0, f64::from(limit.y)) as f32,
                )
            }
            (
                Action::ScrollUp | Action::ScrollDown | Action::ScrollLeft | Action::ScrollRight,
                Some(ActionData::ScrollUnit(unit)),
            ) => {
                let horizontal = matches!(action, Action::ScrollLeft | Action::ScrollRight);
                let extent = if horizontal {
                    element.bounds.size.width
                } else {
                    element.bounds.size.height
                };
                let amount = match unit {
                    ScrollUnit::Item => self.theme.control_height,
                    ScrollUnit::Page => extent,
                };
                let sign = if matches!(action, Action::ScrollUp | Action::ScrollLeft) {
                    -1.0
                } else {
                    1.0
                };
                let mut position = element.scroll;
                let (value, max) = if horizontal {
                    (&mut position.x, limit.x)
                } else {
                    (&mut position.y, limit.y)
                };
                *value = (f64::from(*value) + sign * f64::from(amount)).clamp(0.0, f64::from(max))
                    as f32;
                position
            }
            _ => return Ok(Some(false)),
        };
        self.scroll_to(target, position)?;
        Ok(Some(true))
    }
}
