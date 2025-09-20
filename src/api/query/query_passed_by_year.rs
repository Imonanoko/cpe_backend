use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use base64::{engine::general_purpose, Engine as _};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use std::collections::{BTreeMap, HashMap, HashSet};
use xlsxwriter::Workbook;

use crate::api::lib::is_authorization;

#[derive(Deserialize)]
pub struct PassedByYearForm {
    pub academic_year: u32,
}

#[derive(Serialize)]
struct PassedByYearRow {
    student_id: String,
    name: String,
    total_correct_answers: i32,
    max_correct_answers: i32,
    sessions_joined: String,
}

#[derive(Serialize)]
struct PassedByYearResponse {
    results: Vec<PassedByYearRow>,
    excel_file: String,
}

const PASS_TOTAL_THRESHOLD: i32 = 3;
const PASS_SINGLE_THRESHOLD: i32 = 2;

#[post("/api/query_passed_by_year")]
pub async fn query_passed_by_year(
    req: HttpRequest,
    session: Session,
    db: web::Data<MySqlPool>,
    form: web::Form<PassedByYearForm>,
) -> HttpResponse {
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期");
    }

    let curr_start = NaiveDate::from_ymd_opt((form.academic_year as i32) + 1911, 8, 1).unwrap();
    let curr_end   = NaiveDate::from_ymd_opt((form.academic_year as i32) + 1912, 7, 31).unwrap();
    let history_end = curr_start.pred_opt().unwrap();

    let curr_rows = match sqlx::query!(
        r#"
        SELECT 
            si.StudentID                         AS student_id,
            si.Name                              AS name,
            es.SN                                AS session_sn,
            es.ExamDate                          AS exam_date,
            COALESCE(ea.CorrectAnswersCount, 0)  AS correct
        FROM ExamAttendance ea
        JOIN ExamSessions es ON es.SN = ea.ExamSession_SN
        JOIN StudentInfo  si ON si.StudentID = ea.StudentID
        WHERE ea.IsAbsent = FALSE
          AND ea.IsExcused = FALSE
          AND es.ExamDate >= ? AND es.ExamDate <= ?
        ORDER BY si.StudentID, es.ExamDate
        "#,
        curr_start, curr_end
    )
    .fetch_all(db.get_ref())
    .await
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("查詢本學年度資料失敗: {e}");
            return HttpResponse::InternalServerError().body("查詢本學年度資料失敗");
        }
    };

    struct CurrAgg {
        name: String,
        per_sn_max: HashMap<i64, i32>,
        total: i32,
        maxv: i32,
    }
    let mut curr_map: HashMap<String, CurrAgg> = HashMap::new();
    for row in curr_rows {
        let sid = row.student_id;
        let name = row.name;
        let sn = row.session_sn as i64;
        let cnt = row.correct as i32;

        let entry = curr_map.entry(sid.clone()).or_insert_with(|| CurrAgg {
            name,
            per_sn_max: HashMap::new(),
            total: 0,
            maxv: 0,
        });
        let prev = *entry.per_sn_max.get(&sn).unwrap_or(&0);
        if cnt > prev {
            entry.per_sn_max.insert(sn, cnt);
        }
    }
    for agg in curr_map.values_mut() {
        let mut total = 0;
        let mut maxv = 0;
        for (_sn, c) in agg.per_sn_max.iter() {
            total += *c;
            if *c > maxv { maxv = *c; }
        }
        agg.total = total;
        agg.maxv  = maxv;
    }

    let prev_agg = match sqlx::query!(
        r#"
        SELECT 
            ea.StudentID AS student_id,
            CAST(COALESCE(SUM(ea.CorrectAnswersCount),0) AS SIGNED) AS total_correct,
            CAST(COALESCE(MAX(ea.CorrectAnswersCount),0) AS SIGNED) AS max_correct
        FROM ExamAttendance ea
        JOIN ExamSessions es ON es.SN = ea.ExamSession_SN
        WHERE ea.IsAbsent = FALSE
          AND ea.IsExcused = FALSE
          AND es.ExamDate <= ?
        GROUP BY ea.StudentID
        "#,
        history_end
    )
    .fetch_all(db.get_ref())
    .await
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("查詢歷史(至前一年末)彙總失敗: {e}");
            return HttpResponse::InternalServerError().body("查詢歷史資料失敗");
        }
    };

    let upto_curr_agg = match sqlx::query!(
        r#"
        SELECT 
            ea.StudentID AS student_id,
            CAST(COALESCE(SUM(ea.CorrectAnswersCount),0) AS SIGNED) AS total_correct,
            CAST(COALESCE(MAX(ea.CorrectAnswersCount),0) AS SIGNED) AS max_correct
        FROM ExamAttendance ea
        JOIN ExamSessions es ON es.SN = ea.ExamSession_SN
        WHERE ea.IsAbsent = FALSE
          AND ea.IsExcused = FALSE
          AND es.ExamDate <= ?
        GROUP BY ea.StudentID
        "#,
        curr_end
    )
    .fetch_all(db.get_ref())
    .await
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("查詢歷史(至本年末)彙總失敗: {e}");
            return HttpResponse::InternalServerError().body("查詢歷史資料失敗");
        }
    };

    let mut passed_before_y: HashMap<String, bool> = HashMap::new();
    for r in prev_agg {
        let total = r.total_correct as i32;
        let maxv  = r.max_correct as i32;
        let passed = total >= PASS_TOTAL_THRESHOLD || maxv >= PASS_SINGLE_THRESHOLD;
        passed_before_y.insert(r.student_id, passed);
    }

    let mut passed_by_end_y: HashMap<String, bool> = HashMap::new();
    for r in upto_curr_agg {
        let total = r.total_correct as i32;
        let maxv  = r.max_correct as i32;
        let passed = total >= PASS_TOTAL_THRESHOLD || maxv >= PASS_SINGLE_THRESHOLD;
        passed_by_end_y.insert(r.student_id, passed);
    }

    let mut selected_ids: HashSet<String> = HashSet::new();
    for (sid, &is_passed_end_y) in passed_by_end_y.iter() {
        if !is_passed_end_y { continue; }
        let was_passed_before = *passed_before_y.get(sid).unwrap_or(&false);
        if !was_passed_before {
            selected_ids.insert(sid.clone());
        }
    }

    let all_rows = match sqlx::query!(
        r#"
        SELECT 
            si.StudentID                         AS student_id,
            si.Name                              AS name,
            es.ExamDate                          AS exam_date,
            COALESCE(ea.CorrectAnswersCount, 0)  AS correct
        FROM ExamAttendance ea
        JOIN ExamSessions es ON es.SN = ea.ExamSession_SN
        JOIN StudentInfo  si ON si.StudentID = ea.StudentID
        WHERE ea.IsAbsent = FALSE
          AND ea.IsExcused = FALSE
        ORDER BY si.StudentID, es.ExamDate
        "#
    )
    .fetch_all(db.get_ref())
    .await
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("查詢全歷年資料失敗: {e}");
            return HttpResponse::InternalServerError().body("查詢歷年資料失敗");
        }
    };

    let mut lifetime_map: HashMap<String, BTreeMap<String, i32>> = HashMap::new();
    let mut name_map: HashMap<String, String> = HashMap::new();
    for r in all_rows {
        if !selected_ids.contains(&r.student_id) { continue; }
        name_map.entry(r.student_id.clone()).or_insert(r.name);
        let date_str = r.exam_date.format("%Y-%m-%d").to_string();
        let e = lifetime_map.entry(r.student_id.clone()).or_insert_with(BTreeMap::new);
        let prev = e.get(&date_str).copied().unwrap_or(0);
        let c = r.correct as i32;
        if c > prev {
            e.insert(date_str, c);
        }
    }

    let mut results: Vec<PassedByYearRow> = Vec::new();
    for sid in selected_ids {
        let (total, maxv, name) = if let Some(agg) = curr_map.get(&sid) {
            (agg.total, agg.maxv, agg.name.clone())
        } else {
            (0, 0, name_map.get(&sid).cloned().unwrap_or_default())
        };

        let joined = lifetime_map
            .get(&sid)
            .map(|m| {
                m.iter()
                    .filter(|(_d, &c)| c > 0)
                    .map(|(d, &c)| format!("{}({})", d, c))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();

        results.push(PassedByYearRow {
            student_id: sid,
            name,
            total_correct_answers: total,
            max_correct_answers: maxv,
            sessions_joined: joined,
        });
    }

    results.sort_by(|a, b| a.student_id.cmp(&b.student_id));
    let filepath = format!("./uploads/passed_by_year_{}.xlsx", form.academic_year);
    let workbook = match Workbook::new(&filepath) {
        Ok(wb) => wb,
        Err(e) => {
            eprintln!("建立 Excel 失敗: {e}");
            return HttpResponse::InternalServerError().body("建立 Excel 失敗");
        }
    };
    let mut sheet = workbook.add_worksheet(None).unwrap();
    sheet.write_string(0, 0, "學號", None).unwrap();
    sheet.write_string(0, 1, "姓名", None).unwrap();
    sheet.write_string(0, 2, "累計題數(本學年度)", None).unwrap();
    sheet.write_string(0, 3, "最高題數(本學年度)", None).unwrap();
    sheet.write_string(0, 4, "各場次題數(全歷年)", None).unwrap();

    for (i, row) in results.iter().enumerate() {
        let r = (i + 1) as u32;
        sheet.write_string(r, 0, &row.student_id, None).unwrap();
        sheet.write_string(r, 1, &row.name, None).unwrap();
        sheet.write_number(r, 2, row.total_correct_answers as f64, None).unwrap();
        sheet.write_number(r, 3, row.max_correct_answers as f64, None).unwrap();
        sheet.write_string(r, 4, &row.sessions_joined, None).unwrap();
    }

    if let Err(e) = workbook.close() {
        eprintln!("關閉 Excel 失敗: {e}");
        return HttpResponse::InternalServerError().body("匯出 Excel 失敗");
    }

    let excel_base64 = match std::fs::read(&filepath) {
        Ok(bytes) => general_purpose::STANDARD.encode(bytes),
        Err(e) => {
            eprintln!("讀取 Excel 失敗: {e}");
            return HttpResponse::InternalServerError().body("讀取 Excel 錯誤");
        }
    };
    let _ = std::fs::remove_file(&filepath);

    HttpResponse::Ok().json(PassedByYearResponse { results, excel_file: excel_base64 })
}
