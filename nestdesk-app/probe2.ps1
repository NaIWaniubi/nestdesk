Add-Type @"
using System;using System.Text;using System.Runtime.InteropServices;
public class WinProbe2 {
 [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
 public delegate bool EnumProc(IntPtr h, IntPtr l);
 [DllImport("user32.dll")] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
 [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out int v, int s);
 [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
 public static void Run() {
  EnumWindows((h, l) => {
   var sb = new StringBuilder(64); GetClassNameW(h, sb, 64);
   if (!sb.ToString().Contains("Chrome_WidgetWin") && !sb.ToString().Contains("Tauri") && !sb.ToString().Contains("WRY")) return true;
   RECT r; GetWindowRect(h, out r);
   if (r.R-r.L < 60) return true;
   int cloak=-1, live=1; DwmGetWindowAttribute(h, 14, out cloak, 4); DwmGetWindowAttribute(h, 4, out live, 4);
   Console.WriteLine("hwnd=" + (long)h + " class=" + sb + " vis=" + IsWindowVisible(h) + " iconic=" + IsIconic(h) + " rect=(" + r.L + "," + r.T + "," + r.R + "," + r.B + ") cloaked=" + cloak + " fcVisible=" + (live!=0));
   return true;
  }, IntPtr.Zero);
 }
}
"@
[WinProbe2]::Run()
