import "std/ffi"
import "imgj"
import "pngdec"
"D:/Whisper/apps/wheels/img/big.png" png-decode
// KV: W/H/raw（png 解码）→ img/px（像素打印后 img 未滤波）——直接用 png-size 的像素:
// png-decode 打印像素并留 KV "img"（未滤波 RGB 串）与 W/H
