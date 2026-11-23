Add-Type -AssemblyName System.Drawing
$code = @'
using System;using System.Drawing;using System.Drawing.Imaging;using System.Runtime.InteropServices;
public class GdiShot {
 [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr h);
 [DllImport("user32.dll")] static extern int ReleaseDC(IntPtr h,IntPtr d);
 [DllImport("gdi32.dll")] static extern int GetDeviceCaps(IntPtr d,int i);
 [DllImport("gdi32.dll")] static extern bool BitBlt(IntPtr d,int x,int y,int w,int h,IntPtr s,int sx,int sy,uint r);
 public static void Cap(string outPath) {
  IntPtr sdc = GetDC(IntPtr.Zero);
  int w = GetDeviceCaps(sdc,8), h = GetDeviceCaps(sdc,10);
  Bitmap bmp = new Bitmap(w,h,PixelFormat.Format32bppArgb);
  using (Graphics g = Graphics.FromImage(bmp)) {
   IntPtr ddc = g.GetHdc();
   BitBlt(ddc,0,0,w,h,sdc,0,0,0x00CC0020);
   g.ReleaseHdc(ddc);
  }
  bmp.Save(outPath, ImageFormat.Png);
  ReleaseDC(IntPtr.Zero, sdc);
  Console.WriteLine("saved " + w + "x" + h + " -> " + outPath);
 }
}
'@
Add-Type -TypeDefinition $code -ReferencedAssemblies System.Drawing
[GdiShot]::Cap("$env:TEMP\nest-gdi.png")
