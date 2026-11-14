//! Live checks against the real public services. Ignored by default (they need the network and
//! the providers' goodwill); run them with:
//!   AUTO_MANAGER_LIVE=1 cargo test -p auto-manager-core --test live -- --ignored --test-threads=1
//! Recorded-response tests elsewhere prove the parsers; only these prove the integrations work.

use auto_manager_core::providers::{bulletins, canada, epa, recalls, safety, vpic};
use auto_manager_core::{Core, vehicles};

fn core() -> Option<(Core, tempfile::TempDir)> {
    if std::env::var_os("AUTO_MANAGER_LIVE").is_none() {
        eprintln!("skipped: set AUTO_MANAGER_LIVE=1");
        return None;
    }
    let dir = tempfile::tempdir().expect("temp dir");
    Some((Core::memory(dir.path().to_path_buf()).expect("core"), dir))
}

#[tokio::test]
#[ignore]
async fn live_vpic_decode_and_lists() {
    let Some((core, _d)) = core() else { return };
    let r = vpic::decode(&core, "1HGCM82633A004352", None).await.unwrap();
    assert!(r.value.identified, "{}", r.value.message);
    assert!(r.value.values.iter().any(|v| v.key == "model" && v.value == "Accord"));
    let batch = vpic::decode_batch(&core, &["1HGCM82633A004352".into(), "3N1AB7AP4KY250873".into()]).await.unwrap();
    assert!(batch.iter().all(|d| d.identified));
    assert!(vpic::makes(&core).await.unwrap().names.iter().any(|m| m.eq_ignore_ascii_case("honda")));
    assert!(vpic::models(&core, "honda", 2003).await.unwrap().names.contains(&"Accord".to_string()));
}

#[tokio::test]
#[ignore]
async fn live_recalls_ratings_and_model_specs() {
    let Some((core, _d)) = core() else { return };
    let v = core
        .tx(|c| {
            vehicles::create_manual(
                c,
                &vehicles::ManualVehicle { year: Some(2019), make: "NISSAN".into(), model: "Sentra".into(), ..Default::default() },
            )
        })
        .unwrap();
    let r = recalls::for_vehicle(&core, &v.id).await.unwrap();
    assert!(!r.recalls.is_empty() && r.recalls.iter().all(|x| x.report_date.is_some()));
    let ratings = safety::for_model(&core, 2019, "nissan", "sentra").await.unwrap();
    assert!(ratings.ratings.iter().any(|x| x.overall.is_some()));
    let c = canada::options(&core, 2019, "nissan", "sentra").await.unwrap();
    assert!(c.rows.iter().any(|row| row.values.iter().any(|x| x.key == "wheelbase")));
    let e = epa::options(&core, 2019, "Nissan", "Sentra").await.unwrap();
    assert!(!e.options.is_empty());
    let (vals, _) = epa::vehicle(&core, &e.options[0].id).await.unwrap();
    assert!(vals.iter().any(|x| x.key == "mpg_combined"));
    let b = bulletins::for_model(&core, 2019, "nissan", "sentra").await.unwrap();
    assert!(b.bulletins.len() > 50, "NHTSA lists many communications for the 2019 Sentra");
    let with_doc = b.bulletins.iter().find(|x| x.documents > 0).unwrap();
    let docs = bulletins::documents(&core, with_doc.nhtsa_id).await.unwrap();
    let pdf = bulletins::download(&core, &docs[0].url).await.unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
}
