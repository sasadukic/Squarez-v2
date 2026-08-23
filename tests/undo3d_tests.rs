// tests/undo3d_tests.rs — undo/redo must work for 3D paint and modeling.
use squarez::color::ColorState;
use squarez::history::{Command, UndoStack};
use squarez::project::{Project, ProjectMode};
use squarez::three_d::edit::extrude_faces_n;
use squarez::three_d::mesh::Mesh;
use squarez::three_d::paint::fill_island;

fn cube_project() -> Project {
    let mut p = Project::new_with_mode(128, 128, "u".to_string(), ProjectMode::ThreeD);
    let mut mesh = Mesh::cube(8);
    mesh.allocate_all_islands((128, 128)).unwrap();
    squarez::three_d::paint_islands_checker(&mut p.animations[0].frames[0].layers[0], &mesh);
    p.mesh3d = Some(mesh);
    p
}

#[test]
fn paint_stroke_undo_redo_in_3d() {
    let mut p = cube_project();
    let mut stack = UndoStack::new();
    let mut cs = ColorState::default();
    let isl = p.mesh3d.as_ref().unwrap().faces[0].island;
    let before = p.animations[0].frames[0].layers[0].get_pixel(isl.x as u32, isl.y as u32);

    let edits = fill_island(&mut p.animations[0].frames[0].layers[0], isl, [10, 20, 30, 255]);
    for &(x, y, _, new) in &edits {
        p.animations[0].frames[0].layers[0].set_pixel(x, y, new);
    }
    stack.push(Command::PaintPixels { animation_id: 0, frame_id: 0, layer_id: 0, edits });
    assert_eq!(p.animations[0].frames[0].layers[0].get_pixel(isl.x as u32, isl.y as u32), [10, 20, 30, 255]);

    stack.undo_with_color(&mut p, &mut cs);
    assert_eq!(
        p.animations[0].frames[0].layers[0].get_pixel(isl.x as u32, isl.y as u32),
        before,
        "undo must restore the pre-stroke texel"
    );
    stack.redo_with_color(&mut p, &mut cs);
    assert_eq!(p.animations[0].frames[0].layers[0].get_pixel(isl.x as u32, isl.y as u32), [10, 20, 30, 255]);
}

#[test]
fn extrude_undo_redo_in_3d() {
    let mut p = cube_project();
    let mut stack = UndoStack::new();
    let mut cs = ColorState::default();
    let before = p.mesh3d.clone().unwrap();
    let layer = p.animations[0].frames[0].layers[0].clone();
    let out = extrude_faces_n(&before, &layer, &[1], 2, (128, 128)).expect("fits");

    for &(x, y, _, new) in &out.pixel_edits {
        p.animations[0].frames[0].layers[0].set_pixel(x, y, new);
    }
    p.mesh3d = Some(out.mesh.clone());
    stack.push(Command::MeshEdit {
        before: before.clone(),
        after: out.mesh.clone(),
        canvas_before: (p.canvas_width, p.canvas_height),
        canvas_after: (p.canvas_width, p.canvas_height),
        layer_edits: vec![(0, out.pixel_edits.clone())],
    });
    assert_eq!(p.mesh3d.as_ref().unwrap().faces.len(), 10);

    stack.undo_with_color(&mut p, &mut cs);
    assert_eq!(p.mesh3d.as_ref(), Some(&before), "undo must restore the pre-extrude mesh");
    stack.redo_with_color(&mut p, &mut cs);
    assert_eq!(p.mesh3d.as_ref(), Some(&out.mesh));
}

#[test]
fn undo_restores_a_layout_shifting_edit_texel_for_texel() {
    // Every edit relayouts the whole mesh, so an edit can move islands that
    // have nothing to do with the geometry that changed. Undo has to put the
    // entire atlas back, not just the edited face.
    let mut p = cube_project();
    let atlas = (p.canvas_width, p.canvas_height);

    // Paint every texel distinctly so a misplaced restore is detectable.
    for (i, face) in p.mesh3d.as_ref().unwrap().faces.clone().iter().enumerate() {
        let isl = face.island;
        for y in isl.y..isl.y + isl.h {
            for x in isl.x..isl.x + isl.w {
                let c = [i as u8 + 1, x as u8, y as u8, 255];
                p.animations[0].frames[0].layers[0].set_pixel(x as u32, y as u32, c);
            }
        }
    }
    let mesh_before = p.mesh3d.clone().unwrap();
    let pixels_before = p.animations[0].frames[0].layers[0].pixels.clone();

    // Move a corner outward: the block grows, so islands shift.
    let layer = p.animations[0].frames[0].layers[0].clone();
    let out = squarez::three_d::edit::move_vertices(&mesh_before, &layer, &[0], [-2, 0, -2], atlas)
        .expect("fits");
    for &(x, y, _, new) in &out.pixel_edits {
        p.animations[0].frames[0].layers[0].set_pixel(x, y, new);
    }
    let mut stack = UndoStack::new();
    stack.push(Command::MeshEdit {
        before: mesh_before.clone(),
        after: out.mesh.clone(),
        canvas_before: (p.canvas_width, p.canvas_height),
        canvas_after: (p.canvas_width, p.canvas_height),
        layer_edits: vec![(0, out.pixel_edits.clone())],
    });
    p.mesh3d = Some(out.mesh);
    assert_ne!(
        p.animations[0].frames[0].layers[0].pixels, pixels_before,
        "the edit must actually have moved texels for this test to mean anything"
    );

    stack.undo(&mut p);
    assert_eq!(p.mesh3d.as_ref().unwrap(), &mesh_before, "mesh must be restored");
    assert_eq!(
        p.animations[0].frames[0].layers[0].pixels, pixels_before,
        "every atlas texel must be restored, not just the edited face's"
    );
}
