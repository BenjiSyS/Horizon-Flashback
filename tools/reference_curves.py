"""Generate numeric fixtures by compiling the pinned upstream Java curve code.

No game assets or runtime are used. Minimal dependency stubs supply vectors,
linear arithmetic and angle wrapping; spline/easing code is copied unchanged.
Requires a JDK, the pinned Flashback checkout, and Python 3.12+.
"""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / ".upstream/flashback/src/main/java"
BUILD = ROOT / "target/java-curve-reference"
FILES = [
    "com/moulberry/flashback/spline/CatmullRom.java",
    "com/moulberry/flashback/keyframe/interpolation/SidedInterpolationType.java",
]
STUBS = {
    "com/moulberry/flashback/Flashback.java": "package com.moulberry.flashback; public class Flashback {}",
    "com/moulberry/flashback/Interpolation.java": """
package com.moulberry.flashback;
public class Interpolation {
  public static float linear(float a, float b, float t) { return a+(b-a)*t; }
  public static double linear(double a, double b, double t) { return a+(b-a)*t; }
}
""",
    "net/minecraft/util/Mth.java": """
package net.minecraft.util;
public class Mth {
  public static float wrapDegrees(float v) {
    float r=v%360.0f; if(r>=180.0f) r-=360.0f; if(r< -180.0f) r+=360.0f; return r;
  }
}
""",
    "org/joml/Vector3d.java": """
package org.joml;
public class Vector3d {
  public double x,y,z;
  public Vector3d() {}
  public Vector3d(double x,double y,double z) { this.x=x;this.y=y;this.z=z; }
  public Vector3d(Vector3d v) { this(v.x,v.y,v.z); }
  public Vector3d lerp(Vector3d b,double t,Vector3d d) {
    d.x=x+(b.x-x)*t;d.y=y+(b.y-y)*t;d.z=z+(b.z-z)*t;return d;
  }
}
""",
    "Reference.java": """
import com.moulberry.flashback.spline.CatmullRom;
import com.moulberry.flashback.keyframe.interpolation.SidedInterpolationType;
import org.joml.Vector3d;
public class Reference {
  public static void main(String[] args) {
    float[] amounts={0,.125f,.25f,.5f,.75f,.875f,1};
    var p0=new Vector3d(0,0,0);var p1=new Vector3d(2,4,-1);
    var p2=new Vector3d(12,3,8);var p3=new Vector3d(13,20,8);
    System.out.print("{\\\"smooth\\\":[");
    for(int i=0;i<amounts.length;i++) {
      if(i>0) System.out.print(",");float a=amounts[i];
      var p=CatmullRom.position(p0,p1,p2,p3,3,20,35,a);
      System.out.print("{\\\"amount\\\":"+a+",\\\"scalar\\\":"+CatmullRom.value(10,20,100,0,3,20,35,a)
        +",\\\"angle\\\":"+CatmullRom.degrees(350,5,35,80,3,20,35,a)
        +",\\\"position\\\":["+p.x+","+p.y+","+p.z+"]}");
    }
    System.out.print("],\\\"easing\\\":[");
    String[][] pairs={{"LINEAR","LINEAR"},{"EASE","LINEAR"},{"LINEAR","EASE"},
      {"EASE","EASE"},{"HOLD","LINEAR"},{"SMOOTH","EASE"},{"HERMITE","LINEAR"},
      {"LINEAR","HOLD"},{"SMOOTH","HERMITE"}};
    for(int j=0;j<pairs.length;j++) {
      if(j>0) System.out.print(",");
      var left=SidedInterpolationType.valueOf(pairs[j][0]);
      var right=SidedInterpolationType.valueOf(pairs[j][1]);
      System.out.print("{\\\"left\\\":\\\""+left+"\\\",\\\"right\\\":\\\""+right+"\\\",\\\"values\\\":[");
      for(int i=0;i<amounts.length;i++) { if(i>0) System.out.print(",");
        System.out.print(SidedInterpolationType.interpolate(left,right,amounts[i])); }
      System.out.print("]}");
    }
    System.out.print("]}");
  }
}
""",
}


def main():
    pins = json.loads((ROOT / "compatibility.json").read_text())
    head = subprocess.check_output(["git", "-C", ROOT / ".upstream/flashback", "rev-parse", "HEAD"], text=True).strip()
    if head != pins["flashback_revision"]:
        raise ValueError("Flashback checkout does not match the exact reference pin")
    subprocess.run(["git", "-C", ROOT / ".upstream/flashback", "diff", "--quiet", "HEAD", "--",
                    *["src/main/java/" + name for name in FILES]], check=True)
    javac = shutil.which("javac")
    if not javac:
        raise ValueError("JDK javac is required")
    java = Path(javac).with_name("java.exe" if Path(javac).suffix == ".exe" else "java")
    BUILD.mkdir(parents=True, exist_ok=True)
    hashes = {}
    for name in FILES:
        data = (SOURCE / name).read_bytes()
        hashes[name] = hashlib.sha256(data).hexdigest()
        output = BUILD / name
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(data)
    for name, text in STUBS.items():
        output = BUILD / name
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(text, newline="\n")
    subprocess.run([javac, "-d", str(BUILD / "classes"), *[str(BUILD / name) for name in [*FILES, *STUBS]]], check=True)
    result = json.loads(subprocess.check_output([str(java), "-cp", str(BUILD / "classes"), "Reference"], text=True))
    result["reference"] = {"revision": head, "files": hashes,
                           "scope": "Original CatmullRom and sided-easing code with minimal arithmetic/vector dependency stubs"}
    fixture = ROOT / "tests/fixtures/interpolation-java.json"
    fixture.parent.mkdir(parents=True, exist_ok=True)
    fixture.write_text(json.dumps(result, indent=2) + "\n", newline="\n")
    print(f"Generated {fixture.relative_to(ROOT)} from pinned Java source")


if __name__ == "__main__":
    main()
