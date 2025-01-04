# cpe_backend
東海大學資訊工程學系CPE統計系統後端
# 查看編碼格式
`file -i students.csv`
# 轉換編碼
`iconv -f iso-8859-1 -t utf-8 students.csv -o students_utf8.csv`
* -f：原文件的編碼格式。
* -t：目標文件的編碼格式。
* -o：輸出名稱。