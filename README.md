# cpe_backend
東海大學資訊工程學系CPE統計系統後端
# database
## database name
**cpe_database**
## 在學狀況表 (EnrollmentStatus)
```sql
CREATE TABLE EnrollmentStatus (
    SN INT AUTO_INCREMENT PRIMARY KEY, -- 自動遞增的主鍵
    Status VARCHAR(30) NOT NULL, -- 狀態
    Notes VARCHAR(255) -- 備註欄
);
```
![螢幕擷取畫面 2024-12-29 224825](https://hackmd.io/_uploads/SkX5bk18yg.png)
## 學生屬性表 (StudentAttributes)
```sql
CREATE TABLE StudentAttributes (
    SN INT AUTO_INCREMENT PRIMARY KEY, -- 自動遞增的主鍵
    Attribute VARCHAR(50) NOT NULL, -- 學生屬性
    Notes VARCHAR(255) -- 備註欄
);
```
![image](https://hackmd.io/_uploads/rkD5fkyLyl.png)

## 學生基本資料表 (StudentInfo)
```sql
CREATE TABLE StudentInfo (
    StudentID VARCHAR(20) PRIMARY KEY, -- 學號，主鍵
    Name VARCHAR(50) NOT NULL, -- 姓名
    EnrollmentStatus_SN INT, -- 外來鍵，對應 EnrollmentStatus 的 SN
    StudentAttribute_SN INT, -- 外來鍵，對應 StudentAttributes 的 SN
    IsPassed BOOLEAN DEFAULT FALSE, -- 是否通過，預設否
    PassingCriteria VARCHAR(255) DEFAULT NULL, -- 通過條件，預設為 NULL
    Notes VARCHAR(255), -- 備註
    FOREIGN KEY (EnrollmentStatus_SN) REFERENCES EnrollmentStatus(SN), -- 外來鍵約束
    FOREIGN KEY (StudentAttribute_SN) REFERENCES StudentAttributes(SN) -- 外來鍵約束
);
```
## 考試場次基本資料表 (ExamSessions)
```sql
CREATE TABLE ExamSessions (
    SN INT AUTO_INCREMENT PRIMARY KEY, -- 自動遞增的主鍵
    ExamDate DATE NOT NULL, -- 考試日期
    ExamType ENUM('官辦', '自辦') NOT NULL, -- 考試類別，限定值
    Notes VARCHAR(255) -- 備註欄
);
```
## 應考紀錄表 (ExamAttendance)
```sql
CREATE TABLE ExamAttendance (
    SN INT AUTO_INCREMENT PRIMARY KEY, -- 自動遞增的主鍵
    ExamSession_SN INT NOT NULL, -- 外來鍵，對應 ExamSessions 的 SN
    StudentID VARCHAR(20) NOT NULL, -- 外來鍵，對應 StudentInfo 的 StudentID
    IsAbsent BOOLEAN DEFAULT FALSE, -- 是否缺考，預設否
    IsExcused BOOLEAN DEFAULT FALSE, -- 是否請假，預設否
    CorrectAnswersCount INT DEFAULT 0, -- 答對題數，預設為 0
    Notes VARCHAR(255), -- 備註
    FOREIGN KEY (ExamSession_SN) REFERENCES ExamSessions(SN), -- 外來鍵約束
    FOREIGN KEY (StudentID) REFERENCES StudentInfo(StudentID) -- 外來鍵約束
);
```
## users
```sql
CREATE TABLE users (
    id INT AUTO_INCREMENT PRIMARY KEY,
    username VARCHAR(255) NOT NULL UNIQUE,
    password VARCHAR(255) NOT NULL
);
```
