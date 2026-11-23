$d = [Environment]::GetFolderPath("Desktop")
$files = @(
 "演示-screenshot-001.png","演示-screenshot-002.png","演示-壁纸.png","演示-截图2026.jpg",
 "演示-实验报告.docx","演示-成绩单.pdf","演示-笔记.md","演示-表格.xlsx",
 "演示-installer.exe","演示-绿色版.zip","演示-素材包.7z",
 "演示-script.py","演示-todo.txt","演示-视频.mp4","演示-无名文件.qqxx"
)
foreach ($f in $files) {
  $n = 300 + (Get-Random -Maximum 4000)
  $bytes = New-Object byte[] $n
  (New-Object Random).NextBytes($bytes)
  [IO.File]::WriteAllBytes((Join-Path $d $f), $bytes)
}
Write-Output ("created=" + (@($files | Where-Object { Test-Path (Join-Path $d $_) }).Count) + " at " + $d)
