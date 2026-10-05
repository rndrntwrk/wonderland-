(
    name: "WonderlandSource",
    resources: [
        (name:"colorImage",kind:Texture(kind:Sampler2D,fallback:White),binding:0),
        (name:"depthAlpha",kind:Texture(kind:Sampler2D,fallback:White),binding:1),
        (name:"properties",kind:PropertyGroup([
            (name:"vp0",kind:Vector4(value:(1.0,0.0,0.0,0.0))),
            (name:"vp1",kind:Vector4(value:(0.0,1.0,0.0,0.0))),
            (name:"vp2",kind:Vector4(value:(0.0,0.0,1.0,0.0))),
            (name:"vp3",kind:Vector4(value:(0.0,0.0,0.0,1.0))),
            (name:"color",kind:Vector4(value:(1.0,1.0,1.0,1.0))),
            (name:"lightCutoff",kind:Vector4(value:(1.0,1.0,1.0,0.0))),
            (name:"sprite",kind:Vector4(value:(0.0,0.0,0.0,0.0))),
            (name:"idColor",kind:Vector4(value:(0.0,0.0,0.0,1.0))),
            (name:"passFlags",kind:Vector4(value:(0.0,0.0,0.0,0.0))),
            (name:"unlit",kind:Float(value:1.0)),
        ]),binding:0),
    ],
    disabled_passes:["GBuffer","DirectionalShadow","PointShadow","SpotShadow"],
    passes:[(
        name:"Forward",
        draw_parameters:DrawParameters(
            cull_face:None,
            color_write:ColorMask(red:true,green:true,blue:true,alpha:true),
            depth_write:true,stencil_test:None,depth_test:Some(LessOrEqual),
            blend:Some(BlendParameters(
                func:BlendFunc(sfactor:One,dfactor:OneMinusSrcAlpha,alpha_sfactor:One,alpha_dfactor:OneMinusSrcAlpha),
                equation:BlendEquation(rgb:Add,alpha:Add),
            )),
            stencil_op:StencilOp(fail:Keep,zfail:Keep,zpass:Keep,write_mask:0xFFFFFFFF),
            scissor_box:None,
        ),
        vertex_shader:r#"
            layout(location=0) in vec3 vertexPosition;
            layout(location=1) in vec2 vertexTexCoord;
            layout(location=2) in vec3 vertexNormal;
            layout(location=3) in vec4 sourceColor;
            out vec2 uv;
            out vec3 normal;
            out vec4 color;
            void main(){
                mat4 vp=mat4(properties.vp0,properties.vp1,properties.vp2,properties.vp3);
                vec4 clip=vp*vec4(vertexPosition,1.0);
                if(properties.sprite.z>0.5)clip=vec4(vertexPosition,1.0);
                // C clip depth 0..1 becomes OpenGL -1..1 once.
                clip.z=2.0*clip.z-clip.w;
                gl_Position=clip;uv=vertexTexCoord;normal=vertexNormal;color=sourceColor;
            }
        "#,
        fragment_shader:r#"
            in vec2 uv;
            in vec3 normal;
            in vec4 color;
            out vec4 FragColor;
            vec3 sourceToLinear(vec3 c){return mix(c/12.92,pow((c+0.055)/1.055,vec3(2.4)),greaterThan(c,vec3(0.04045)));}
            void main(){
                vec4 texel=texture(colorImage,uv)*properties.color*color;
                vec4 aux=texture(depthAlpha,uv);
                float depth=gl_FragCoord.z;
                bool sprite=properties.sprite.z>0.5;
                if(sprite){texel.a=aux.g;depth=properties.sprite.x+(1.0-aux.r)/0.4*(properties.sprite.y-properties.sprite.x);}
                if(depth<0.0||depth>1.0)discard;
                gl_FragDepth=depth;
                float byteAlpha=floor(texel.a*255.0+0.5);
                if(properties.passFlags.x>0.5){
                    if(byteAlpha<26.0||texel.a<properties.lightCutoff.a)discard;
                    // Ownerless surfaces write zero ID plus depth to preserve occlusion.
                    FragColor=vec4(sourceToLinear(properties.idColor.rgb),1.0);return;
                }
                if(byteAlpha<=2.0||texel.a<properties.lightCutoff.a)discard;
                vec3 rgb=texel.rgb;
                if(sprite){
                    int room=int(properties.sprite.w);
                    if(room==65534)rgb=vec3(1.0)-rgb;
                    else if(room==65533)rgb=vec3(dot(rgb,vec3(0.2989,0.587,0.114)));
                    else if(room!=65535&&room%256!=0)rgb=pow(pow(rgb,vec3(2.2))*properties.lightCutoff.rgb,vec3(1.0/2.2));
                }else if(properties.unlit<0.5){rgb*=0.35+0.65*max(dot(normalize(normal),normalize(vec3(0.4,1.0,0.25))),0.0);}
                // Source gamma lighting is explicit; final target blending/display transfer must be measured.
                FragColor=vec4(sourceToLinear(rgb)*texel.a,texel.a);
            }
        "#,
    )],
)
