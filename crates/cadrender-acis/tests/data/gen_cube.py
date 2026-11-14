"""Make cube.sat and cube.sab (a 2 x 2 x 2 cube at (10, 20, 0), moved by a
body transform) with ezdxf:  python gen_cube.py cube.sat cube.sab"""
import sys
from ezdxf.acis import api
from ezdxf.render import forms

cube = forms.cube().scale(2, 2, 2).translate(10, 20, 0)
body = api.body_from_mesh(cube)
sat = api.export_sat([body], version=700)
open(sys.argv[1], "w").write("\n".join(sat) + "\n")
open(sys.argv[2], "wb").write(api.export_sab([body]))
