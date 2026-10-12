//! Where the frames around the editor go: a column on its left, one on its right,
//! each holding frames one above the other in the order chosen.

/// A frame that can be placed in one of the two columns beside the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Frame {
    /// Name and logo of the app.
    Brand,
    /// Name of the open project, with the menu of the known projects.
    Project,
    /// Git state of the project and its buttons (git extension).
    Git,
    /// List of the pages, with its filter.
    Pages,
    /// Button opening the settings.
    Settings,
    Backlinks,
    Tags,
    /// Headlines of the project with a TODO keyword.
    Todos,
    /// Pages linked to but not created.
    Broken,
    /// Button quitting the app.
    Quit,
    /// Button building the site of the project (org-bloom extension).
    OrgBloom,
}

impl Frame {
    pub const ALL: [Frame; 11] = [
        Frame::Brand, Frame::Project, Frame::Git, Frame::OrgBloom, Frame::Pages, Frame::Settings,
        Frame::Backlinks, Frame::Tags, Frame::Todos, Frame::Broken, Frame::Quit,
    ];

    /// Its name in `settings.json`.
    pub fn id(self) -> &'static str {
        match self {
            Frame::Brand => "brand",
            Frame::Project => "project",
            Frame::Git => "git",
            Frame::Pages => "pages",
            Frame::Settings => "settings",
            Frame::Backlinks => "backlinks",
            Frame::Tags => "tags",
            Frame::Todos => "todos",
            Frame::Broken => "broken",
            Frame::Quit => "quit",
            Frame::OrgBloom => "org_bloom",
        }
    }

    pub fn from_id(id: &str) -> Option<Frame> {
        Frame::ALL.into_iter().find(|f| f.id() == id)
    }

    /// Whether it takes a share of the height of its column (a list), rather
    /// than only the height of its content.
    pub fn grows(self) -> bool {
        matches!(self, Frame::Pages | Frame::Backlinks | Frame::Tags | Frame::Todos | Frame::Broken)
    }

    /// The column it is in when none was chosen.
    fn default_right(self) -> bool {
        matches!(self, Frame::Backlinks | Frame::Tags | Frame::Todos | Frame::Broken | Frame::Quit)
    }
}

/// How a frame is moved in the layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// Above the frame before it.
    Up,
    /// Below the frame after it.
    Down,
    /// To the end of the other column.
    OtherColumn,
}

/// The frames of each column, top to bottom. Each frame is in exactly one of
/// them, shown or not (whether it is shown is a setting of its own).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub left: Vec<Frame>,
    pub right: Vec<Frame>,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            left: vec![Frame::Brand, Frame::Project, Frame::Git, Frame::OrgBloom, Frame::Pages, Frame::Settings],
            right: vec![Frame::Quit, Frame::Backlinks, Frame::Tags, Frame::Todos, Frame::Broken],
        }
    }
}

impl Layout {
    /// The layout saved as these lists of names. Unknown names and repeats are
    /// dropped; a frame in neither list (one added by a later version) goes to
    /// the end of its usual column.
    pub fn from_ids(left: Option<&[String]>, right: Option<&[String]>) -> Self {
        if left.is_none() && right.is_none() {
            return Self::default();
        }
        let mut seen = vec![];
        let mut read = |ids: Option<&[String]>| -> Vec<Frame> {
            ids.unwrap_or_default().iter().filter_map(|id| Frame::from_id(id))
                .filter(|f| if seen.contains(f) { false } else { seen.push(*f); true })
                .collect()
        };
        let mut layout = Self { left: read(left), right: read(right) };
        for f in Frame::ALL {
            if !seen.contains(&f) {
                if f.default_right() { layout.right.push(f) } else { layout.left.push(f) }
            }
        }
        layout
    }

    pub fn ids(frames: &[Frame]) -> Vec<String> {
        frames.iter().map(|f| f.id().to_string()).collect()
    }

    pub fn column(&self, right: bool) -> &[Frame] {
        if right { &self.right } else { &self.left }
    }

    /// The column holding `frame` (true: the right one), and its place there.
    pub fn find(&self, frame: Frame) -> Option<(bool, usize)> {
        self.left.iter().position(|f| *f == frame).map(|i| (false, i))
            .or_else(|| self.right.iter().position(|f| *f == frame).map(|i| (true, i)))
    }

    /// Move `frame`. Up or down, it goes past the next frame not `hidden` (one
    /// the settings don't list). Does nothing when it can't go further.
    pub fn move_frame(&mut self, frame: Frame, how: Move, hidden: impl Fn(Frame) -> bool) {
        let Some((right, i)) = self.find(frame) else { return };
        let col = if right { &mut self.right } else { &mut self.left };
        match how {
            Move::Up => if let Some(j) = (0..i).rev().find(|&j| !hidden(col[j])) {
                let f = col.remove(i);
                col.insert(j, f);
            },
            Move::Down => if let Some(j) = (i + 1..col.len()).find(|&j| !hidden(col[j])) {
                let f = col.remove(i);
                col.insert(j, f);
            },
            Move::OtherColumn => {
                let f = col.remove(i);
                if right { self.left.push(f) } else { self.right.push(f) }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn nothing_saved_is_the_default() {
        assert_eq!(Layout::from_ids(None, None), Layout::default());
        let d = Layout::default();
        let back = Layout::from_ids(Some(&Layout::ids(&d.left)), Some(&Layout::ids(&d.right)));
        assert_eq!(back, d);
    }

    #[test]
    fn unknown_repeated_and_missing_frames() {
        let l = Layout::from_ids(Some(&ids(&["git", "nope", "pages"])), Some(&ids(&["broken", "git"])));
        assert_eq!(&l.left[..2], &[Frame::Git, Frame::Pages]);
        assert_eq!(l.right[0], Frame::Broken);
        assert_eq!(l.left.iter().chain(&l.right).count(), Frame::ALL.len());
        assert!(!l.right.contains(&Frame::Git));
        // The missing ones went to their usual column
        assert!(l.left.contains(&Frame::Brand) && l.right.contains(&Frame::Tags));
    }

    #[test]
    fn moves() {
        let mut l = Layout::default();
        l.move_frame(Frame::Git, Move::OtherColumn, |_| false);
        assert_eq!(l.right.last(), Some(&Frame::Git));
        assert!(!l.left.contains(&Frame::Git));
        l.move_frame(Frame::Git, Move::Up, |_| false);
        assert_eq!(&l.right[3..], &[Frame::Todos, Frame::Git, Frame::Broken]);
        l.move_frame(Frame::Git, Move::Down, |_| false);
        l.move_frame(Frame::Git, Move::Down, |_| false);
        assert_eq!(l.right.last(), Some(&Frame::Git));
        // Past a hidden frame: next to the frame after it
        l.move_frame(Frame::Git, Move::Up, |f| f == Frame::Broken);
        assert_eq!(&l.right[3..], &[Frame::Git, Frame::Todos, Frame::Broken]);
        // Already at the top: stays
        l.move_frame(Frame::Quit, Move::Up, |_| false);
        assert_eq!(l.right[0], Frame::Quit);
    }
}
