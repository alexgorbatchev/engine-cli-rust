use engine_library::performance::{
    BeatData, BeatMarker, CueData, LoopData, q_compress, q_uncompress,
};

#[test]
fn native_beat_grid_retains_sample_positions_and_header_layout() {
    let native = include_bytes!("fixtures/beat-data.bin");
    let beat = BeatData::decode(native).unwrap();
    assert_eq!(beat.sample_rate, 44100.0);
    assert_eq!(beat.original.len(), 2);
    assert_eq!(beat.original[0].beat, -4);
    assert!(beat.original[0].samples < 0.0);
    assert_eq!(beat.encode().unwrap(), native);
    let mut changed = beat.clone();
    changed.adjusted[0] = BeatMarker {
        samples: 22050.0,
        beat: 0,
        beats_to_next: 4,
        unknown: 0,
    };
    changed.adjusted[1].samples = 110250.0;
    assert_eq!(
        BeatData::decode(&changed.encode().unwrap()).unwrap(),
        changed
    );
    assert!(BeatData::decode(&native[..native.len() - 20]).is_err());
    changed.sample_rate = f64::NAN;
    assert!(changed.encode().is_err());
}

#[test]
fn empty_native_slots_and_unnamed_cues_are_not_confused() {
    let native = include_bytes!("fixtures/quick-cues.bin");
    let mut cues = CueData::decode(native).unwrap();
    assert!(cues.slots.iter().all(Option::is_none));
    assert_eq!(cues.encode().unwrap(), native);
    cues.slots[3] = Some(engine_library::performance::Cue {
        name: String::new(),
        samples: 0.0,
        color: [255, 1, 2, 3],
    });
    assert_eq!(CueData::decode(&cues.encode().unwrap()).unwrap(), cues);
    let loops = LoopData::decode(include_bytes!("fixtures/loops.bin")).unwrap();
    assert!(loops.slots.iter().all(Option::is_none));
    assert_eq!(
        loops.encode().unwrap(),
        include_bytes!("fixtures/loops.bin")
    );
    assert!(CueData::decode(&native[..100]).is_err());
    assert!(LoopData::decode(&[0; 8]).is_err());
}

#[test]
fn q_compression_checks_declared_and_actual_size_with_a_bound() {
    let compressed = q_compress(b"native performance").unwrap();
    assert_eq!(
        q_uncompress(&compressed, 100).unwrap(),
        b"native performance"
    );
    assert!(q_uncompress(&compressed, 1).is_err());
    assert!(q_uncompress(&[0, 0], 100).is_err());
    let mut mismatch = compressed.clone();
    mismatch[3] -= 1;
    assert!(q_uncompress(&mismatch, 100).is_err());
    let mut bomb = q_compress(&vec![0; 1000]).unwrap();
    bomb[..4].copy_from_slice(&1_u32.to_be_bytes());
    assert!(q_uncompress(&bomb, 32).is_err());
    for length in 0..compressed.len() {
        assert!(
            q_uncompress(&compressed[..length], 100).is_err(),
            "accepted truncated zlib at {length}"
        );
    }
    let mut trailing = compressed.clone();
    trailing.push(0);
    assert!(q_uncompress(&trailing, 100).is_err());
}

#[test]
fn nonempty_loops_unicode_names_and_performance_rows_round_trip() {
    use engine_library::performance::{Loop, PerformanceData};
    let mut loops = LoopData::default();
    loops.slots[7] = Some(Loop {
        name: "桥".into(),
        start: 44100.25,
        end: 88200.5,
        flags: [1, 2, 3],
        color: [7, 8, 9],
    });
    loops.trailing = vec![55, 66];
    let mut cues = CueData::default();
    cues.slots[0] = Some(engine_library::performance::Cue {
        name: "入口".into(),
        samples: 12.5,
        color: [1, 2, 3, 4],
    });
    let native = BeatData::decode(include_bytes!("fixtures/beat-data.bin")).unwrap();
    let row = PerformanceData {
        beat_data: Some(q_compress(&native.encode().unwrap()).unwrap()),
        quick_cues: Some(q_compress(&cues.encode().unwrap()).unwrap()),
        loops: Some(loops.encode().unwrap()),
        ..PerformanceData::default()
    };
    let decoded = row.decode(4096).unwrap();
    assert_eq!(decoded.beats, Some(native));
    assert_eq!(decoded.cues, Some(cues.clone()));
    assert_eq!(decoded.loops, Some(loops.clone()));
    assert!(row.decode(1).is_err());
    cues.slots[0].as_mut().unwrap().name = "x".repeat(256);
    assert!(cues.encode().is_err());
    cues.slots[0].as_mut().unwrap().name.clear();
    cues.slots[0].as_mut().unwrap().samples = -1.0;
    assert!(cues.encode().is_err());
    loops.slots[7].as_mut().unwrap().end = 1.0;
    assert!(loops.encode().is_err());
}
