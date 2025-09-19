use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use base64::{engine::general_purpose, Engine as _};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use std::collections::{BTreeMap, HashMap};
use xlsxwriter::Workbook;

use crate::api::lib::is_authorization;

#[derive(Deserialize)]
pub struct PassedByYearForm {
    /// 例如：113
    pub academic_year: u32,
}

#[derive(Serialize, Clone)]
struct PerSessionEntry {
    exam_date: String,     // "YYYY-MM-DD"
    correct: i32,          // 該場次題數
}

#[derive(Serialize)]
struct PassedByYearRow {
    student_id: String,
    name: String,
    total_correct_answers: i32,
    max_correct_answers: i32,
    passed: bool,
    // 以日期為 key 方便前端（也可回傳陣列）
    per_session: BTreeMap<String, i32>,
}

#[derive(Serialize)]
struct PassedByYearResponse {
    results: Vec<PassedByYearRow>,
    excel_file: String, // base64 xlsx（含動態場次欄）
}

// —— 通過規則（示意）：滿足其一即通過 ——
//   1) 本學年度累計題數 >= 200
//   2) 本學年度單一場次最高題數 >= 80
const PASS_TOTAL_THRESHOLD: i32 = 200;
const PASS_MAX_THRESHOLD: i32 = 80;

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

    // 學年度區間：eg. 113 => 2024-08-01 ~ 2025-07-31
    let curr_start = NaiveDate::from_ymd_opt((form.academic_year as i32) + 1911, 8, 1).unwrap();
    let curr_end   = NaiveDate::from_ymd_opt((form.academic_year as i32) + 1912, 7, 31).unwrap();

    let prev_start = NaiveDate::from_ymd_opt((form.academic_year as i32) + 1910, 8, 1).unwrap();
    let prev_end   = NaiveDate::from_ymd_opt((form.academic_year as i32) + 1911, 7, 31).unwrap();

    // 1) 取本學年度所有場次（作為 Excel 動態欄位），依日期排序
    let sessions_curr = match sqlx::query!(
        r#"
        SELECT SN, ExamDate
        FROM ExamSessions
        WHERE ExamDate >= ? AND ExamDate <= ?
        ORDER BY ExamDate ASC
        "#,
        curr_start, curr_end
    )
    .fetch_all(db.get_ref())
    .await
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("查詢 ExamSessions 失敗: {e}");
            return HttpResponse::InternalServerError().body("查詢場次失敗");
        }
    };
    // 動態欄：以日期字串為主鍵，並保留欄序
    let mut session_cols: Vec<(i64, String)> = Vec::new(); // (SN, "YYYY-MM-DD")
    for s in &sessions_curr {
        if let (Some(sn), Some(date)) = (s.SN, s.ExamDate) {
            session_cols.push((sn as i64, date.format("%Y-%m-%d").to_string()));
        }
    }

    // 2) 取本學年度 每學生 x 每場次 的題數（過濾缺考/請假）
    //    以及學生姓名；後續在程式端累加 total / max 與 pivot
    let curr_rows = match sqlx::query!(
        r#"
        SELECT 
            si.StudentID         AS student_id,
            si.Name              AS name,
            es.SN                AS session_sn,
            es.ExamDate          AS exam_date,
            ea.CorrectAnswersCount AS correct
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
            eprintln!("查詢本學年度 per-session 題數失敗: {e}");
            return HttpResponse::InternalServerError().body("查詢本學年度資料失敗");
        }
    };

    // 3) 取上學年度 聚合（SUM / MAX），過濾缺考/請假
    let prev_agg = match sqlx::query!(
        r#"
        SELECT 
            ea.StudentID AS student_id,
            COALESCE(SUM(ea.CorrectAnswersCount), 0) AS total_correct,
            COALESCE(MAX(ea.CorrectAnswersCount), 0) AS max_correct
        FROM ExamAttendance ea
        JOIN ExamSessions es ON es.SN = ea.ExamSession_SN
        WHERE ea.IsAbsent = FALSE
          AND ea.IsExcused = FALSE
          AND es.ExamDate >= ? AND es.ExamDate <= ?
        GROUP BY ea.StudentID
        "#,
        prev_start, prev_end
    )
    .fetch_all(db.get_ref())
    .await
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("查詢上學年度彙總失敗: {e}");
            return HttpResponse::InternalServerError().body("查詢上學年度資料失敗");
        }
    };

    // 將上學年度通過與否查表化
    let mut prev_pass: HashMap<String, bool> = HashMap::new();
    for r in prev_agg {
        let total = r.total_correct.unwrap_or(0);
        let maxv  = r.max_correct.unwrap_or(0);
        let passed = total >= PASS_TOTAL_THRESHOLD || maxv >= PASS_MAX_THRESHOLD;
        prev_pass.insert(r.student_id.unwrap_or_default(), passed);
    }

    // 4) 整理本學年度：同一學生聚合 + pivot per-session
    // student_id -> (name, per_session_map(date->count), total, max)
    struct CurrAgg {
        name: String,
        per_session: HashMap<i64, i32>, // session_sn -> count
        total: i32,
        maxv: i32,
        per_session_by_date: BTreeMap<String, i32>, // 之後填好 Excel / JSON 用
    }

    let mut curr_map: HashMap<String, CurrAgg> = HashMap::new();
    for row in curr_rows {
        let sid  = row.student_id.unwrap_or_default();
        let name = row.name.unwrap_or_default();
        let sn   = row.session_sn.unwrap_or(0) as i64;
        let cnt  = row.correct.unwrap_or(0);

        let entry = curr_map.entry(sid.clone()).or_insert_with(|| CurrAgg{
            name,
            per_session: HashMap::new(),
            total: 0,
            maxv: 0,
            per_session_by_date: BTreeMap::new(),
        });

        // 以 session_sn 累計（若同場次多筆，以合計或最大？這裡取最大，避免重複計錄把成績加總異常）
        let prev = *entry.per_session.get(&sn).unwrap_or(&0);
        if cnt > prev {
            entry.per_session.insert(sn, cnt);
        }
        // 之後再一次性算 total / max
    }

    // 把 total / max 與 date-map 補齊
    for (_sid, agg) in curr_map.iter_mut() {
        let mut total = 0;
        let mut maxv  = 0;

        // 預先把所有本學年度場次欄位建起來（沒有成績就 0）
        for (sn, date_str) in &session_cols {
            let c = *agg.per_session.get(sn).unwrap_or(&0);
            agg.per_session_by_date.insert(date_str.clone(), c);
            total += c;
            if c > maxv { maxv = c; }
        }

        agg.total = total;
        agg.maxv  = maxv;
    }

    // 5) 篩選：本學年度通過 && 上學年度未通過（或無紀錄視為未通過）
    let mut results: Vec<PassedByYearRow> = Vec::new();
    for (sid, agg) in curr_map.iter() {
        let curr_passed = agg.total >= PASS_TOTAL_THRESHOLD || agg.maxv >= PASS_MAX_THRESHOLD;
        if !curr_passed {
            continue;
        }
        let prev_ok = prev_pass.get(sid).copied().unwrap_or(false);
        if prev_ok {
            continue; // 上年度也通過 → 不要
        }

        results.push(PassedByYearRow {
            student_id: sid.clone(),
            name: agg.name.clone(),
            total_correct_answers: agg.total,
            max_correct_answers: agg.maxv,
            passed: true,
            per_session: agg.per_session_by_date.clone(), // key = "YYYY-MM-DD"
        });
    }

    // 依學號排序（可依需求改）
    results.sort_by(|a, b| a.student_id.cmp(&b.student_id));

    // 6) 產生 Excel：動態欄位 = 本學年度每個場次（按日期）
    let filepath = format!("./uploads/passed_by_year_{}_diff.xlsx", form.academic_year);
    let workbook = match Workbook::new(&filepath) {
        Ok(wb) => wb,
        Err(e) => {
            eprintln!("建立 Excel 失敗: {e}");
            return HttpResponse::InternalServerError().body("建立 Excel 失敗");
        }
    };
    let mut sheet = workbook.add_worksheet(None).unwrap();

    // 固定欄
    sheet.write_string(0, 0, "學號", None).unwrap();
    sheet.write_string(0, 1, "姓名", None).unwrap();
    sheet.write_string(0, 2, "累計題數", None).unwrap();
    sheet.write_string(0, 3, "最高題數", None).unwrap();

    // 動態場次欄：從第 4 欄開始（index 4）
    // 標題用日期 "YYYY-MM-DD"
    for (idx, (_sn, date_str)) in session_cols.iter().enumerate() {
        let col = (4 + idx) as u16;
        sheet.write_string(0, col, date_str, None).unwrap();
    }

    // 寫資料
    for (i, row) in results.iter().enumerate() {
        let r = (i + 1) as u32;
        sheet.write_string(r, 0, &row.student_id, None).unwrap();
        sheet.write_string(r, 1, &row.name, None).unwrap();
        sheet.write_number(r, 2, row.total_correct_answers as f64, None).unwrap();
        sheet.write_number(r, 3, row.max_correct_answers as f64, None).unwrap();

        for (idx, (_sn, date_str)) in session_cols.iter().enumerate() {
            let col = (4 + idx) as u16;
            let v = row.per_session.get(date_str).copied().unwrap_or(0);
            sheet.write_number(r, col, v as f64, None).unwrap();
        }
    }

    if let Err(e) = workbook.close() {
        eprintln!("關閉 Excel 失敗: {e}");
        return HttpResponse::InternalServerError().body("匯出 Excel 失敗");
    }

    // 讀檔 → base64
    let excel_base64 = match std::fs::read(&filepath) {
        Ok(bytes) => general_purpose::STANDARD.encode(bytes),
        Err(e) => {
            eprintln!("讀取 Excel 失敗: {e}");
            return HttpResponse::InternalServerError().body("讀取 Excel 錯誤");
        }
    };
    let _ = std::fs::remove_file(&filepath);

    HttpResponse::Ok().json(PassedByYearResponse {
        results,
        excel_file: excel_base64,
    })
}
