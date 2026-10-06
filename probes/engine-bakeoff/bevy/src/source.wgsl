// Source textures are raw UNORM; source gamma is explicit and never sampled twice as sRGB.
struct Parameters {
    view_projection:mat4x4<f32>, color:vec4<f32>, light_cutoff:vec4<f32>,
    sprite:vec4<f32>, id_color:vec4<f32>, flags:vec4<f32>,
};
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> p:Parameters;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var color_image:texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var color_sampler:sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var depth_alpha:texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var depth_sampler:sampler;
struct VertexInput {
    @location(0) position:vec3<f32>, @location(1) normal:vec3<f32>,
    @location(2) uv:vec2<f32>, @location(3) color:vec4<f32>,
};
struct VertexOutput {
    @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>,
    @location(1) color:vec4<f32>, @location(2) normal:vec3<f32>,
};
@vertex fn vertex(v:VertexInput)->VertexOutput {
    var out:VertexOutput;
    var clip=p.view_projection*vec4(v.position,1.0);
    if p.sprite.z>0.5 {clip=vec4(v.position,1.0);}
    // C uses finite 0..1 forward depth. Both geometry and sprites reverse it once for Bevy.
    clip.z=clip.w-clip.z;
    out.position=clip;out.uv=v.uv;out.color=v.color;out.normal=v.normal;return out;
}
struct FragmentOutput { @location(0) color:vec4<f32>, @builtin(frag_depth) depth:f32 };
@fragment fn fragment(v:VertexOutput)->FragmentOutput {
    var texel=textureSample(color_image,color_sampler,v.uv)*p.color*v.color;
    let aux=textureSample(depth_alpha,depth_sampler,v.uv);
    let sprite=p.sprite.z>0.5;
    var depth=v.position.z;
    if sprite {texel.a=aux.g; depth=1.0-(p.sprite.x+(1.0-aux.r)/0.4*(p.sprite.y-p.sprite.x));}
    if depth<0.0 || depth>1.0 {discard;}
    let byte_alpha=floor(texel.a*255.0+0.5);
    if p.flags.x>0.5 {
        if byte_alpha<26.0 || texel.a<p.light_cutoff.a {discard;}
        // Ownerless geometry writes opaque zero ID and still occludes objects behind it.
        return FragmentOutput(vec4(p.id_color.rgb,1.0),depth);
    }
    if byte_alpha<=2.0 || texel.a<p.light_cutoff.a {discard;}
    var rgb=texel.rgb;
    if sprite {
        let room=u32(p.sprite.w);
        if room==65534u {rgb=vec3(1.0)-rgb;}
        else if room==65533u {rgb=vec3(dot(rgb,vec3(0.2989,0.587,0.114)));}
        else if room!=65535u && room%256u!=0u {rgb=pow(pow(rgb,vec3(2.2))*p.light_cutoff.rgb,vec3(1.0/2.2));}
    } else if p.flags.y<0.5 {
        rgb*=0.35+0.65*max(dot(normalize(v.normal),normalize(vec3(0.4,1.0,0.25))),0.0);
    }
    // CompositingSpace::Srgb keeps the intermediate encoded for source-space blending.
    // Bevy converts to the display attachment only in its final blit.
    return FragmentOutput(vec4(rgb*texel.a,texel.a),depth);
}
