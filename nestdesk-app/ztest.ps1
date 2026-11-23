Add-Type @"
using System;using System.Runtime.InteropServices;
public class ZTest {
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x,int y,int cx,int cy,uint flags);
 public const uint NOMOVE=0x2, NOSIZE=0x1, NOACTIVATE=0x10;
 public static void Top(long h)   { SetWindowPos(new IntPtr(h), new IntPtr(0), 0,0,0,0, NOMOVE|NOSIZE|NOACTIVATE); }
 public static void Bottom(long h){ SetWindowPos(new IntPtr(h), new IntPtr(1), 0,0,0,0, NOMOVE|NOSIZE|NOACTIVATE); }
}
"@
param() # no-op
# usage: ztest.ps1 <hwnd> <top|bottom>
