# Motion

The Motion workbench stores named box/cylinder/ellipsoid objects with sizes, colors and ordered keyframes. Keys retain time, translation, Euler rotation in degrees and positive XYZ scale. Duration and frame rate define playback sampling; scrubbing or playback changes the viewed time without adding edit history.

By default, translation and scale interpolate linearly. Each Euler angle follows its shortest arc; an exact 180° tie follows the negative direction. Geometry is locally scaled, rotated about X/Y/Z in that order, then translated into its parent space. Ancestor transforms compose through up to 64 levels, retaining shear induced by nonuniform parent scales. Changing parent retains local keys and may change world positions. Cycles and missing parents are refused; deleting an object deletes its descendants in one undo step. Evaluation clamps outside times to the scene range, and the first/last keys hold their poses. A native save preserves colors, keys and parent references.

```sh
./target/desktop/debug/forge workbench export target/suite.forge.json target/frame-at-two.obj --area motion --time 2
```

Evaluated OBJ contains geometry at one time. It does not encode keyframes, colors/materials, scene hierarchy or animation. The current scene has up to 1,000 objects and 10,000 total keyframes, with bounded frame counts. General deformation modifiers, rigging, constraints, dynamics, lighting/shader materials, production rendering, compositing and MAX/MA/MB/FBX exchange remain unfinished.


Objects support Linear, Step and Smoothstep interpolation in the inspector. Step holds the left key until the next authored key; Smoothstep uses an eased interval for position, scale and shortest Euler rotations. Visibility is inherited: hiding a parent hides its descendants without deleting keys.

The editable local-space modifier stack supplies arrays and X/Y/Z mirror planes. Arrays include the original copy. Mirroring reverses face winding so reflected geometry stays outward-facing; Keep original retains both copies. Modifiers apply in list order before object and parent transforms. Use Up/Remove to adjust the stack, then Apply object properties. Limits are eight modifiers and 128 combined copies per object, with a scene-wide derived triangle budget of 5,000,000. Overlapping copies remain separate surfaces; these are not boolean unions.

[Advanced workflows](Advanced-workflows) · [Feature status](Feature-status)
