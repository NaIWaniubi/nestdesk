param([int]$X,[int]$Y,[int]$W,[int]$H,[string]$Tag)
Add-Type -AssemblyName System.Drawing
$code = @'
using System;using System.Drawing;using System.Drawing.Imaging;using System.Runtime.InteropServices;
public class PixStats {
 [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr h);
 [DllImport("user32.dll")] static extern int ReleaseDC(IntPtr h,IntPtr d);
 [DllImport("gdi32.dll")] static extern bool BitBlt(IntPtr d,int x,int y,int w,int h,IntPtr s,int sx,int sy,uint r);
 public static string Stats(int rx,int ry,int rw,int rh) {
  IntPtr sdc = GetDC(IntPtr.Zero);
  Bitmap bmp = new Bitmap(rw,rh,PixelFormat.Format32bppArgb);
  using (Graphics g = Graphics.FromImage(bmp)) {
   IntPtr ddc = g.GetHdc();
   BitBlt(ddc,0,0,rw,rh,sdc,rx,ry,0x00CC0020);
   g.ReleaseHdc(ddc);
  }
  int n = rw*rh;
  long sr=0,sg=0,sb=0;
  BitmapData bd = bmp.LockBits(new Rectangle(0,0,rw,rh), ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
  int stride = bd.Stride;
  byte[] buf = new byte[stride*rh];
  Marshal.Copy(bd.Scan0, buf, 0, buf.Length);
  for (int y=0;y<rh;y++) {
   int off = y*stride;
   for (int x=0;x<rw;x++) { int i=off+x*4; sb+=buf[i]; sg+=buf[i+1]; sr+=buf[i+2]; }
  }
  bmp.UnlockBits(bd);
  ReleaseDC(IntPtr.Zero, sdc);
  double mr=(double)sr/n, mg=(double)sg/n, mb=(double)sb/n;
  return string.Format("R={0:F1} G={1:F1} B={2:F1}", mr, mg, mb);
 }
}
'@
Add-Type -TypeDefinition $code -ReferencedAssemblies System.Drawing
$s = [PixStats]::Stats($X,$Y,$W,$H)
Write-Output ("{0} rect=({1},{2},{3}x{4}) meanRGB: {5}" -f $Tag,$X,$Y,$W,$H,$s)