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
        layer_id: 0,
        pixel_edits: out.pixel_edits.clone(),
    });
    assert_eq!(p.mesh3d.as_ref().unwrap().faces.len(), 10);

    stack.undo_with_color(&mut p, &mut cs);
    assert_eq!(p.mesh3d.as_ref(), Some(&before), "undo must restore the pre-extrude mesh");
    stack.redo_with_color(&mut p, &mut cs);
    assert_eq!(p.mesh3d.as_ref(), Some(&out.mesh));
}
