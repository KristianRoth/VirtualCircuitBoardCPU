import sys
def block(rows, probes):
    w=max(len(r) for r in rows); return ([r.ljust(w,'.') for r in rows], probes)
def col(chars, label):   # 3-wide column, chars top to bottom, LED block appended
    rows=['.'+c+'.' for c in chars]+['EEE']*3
    return block(rows,[(label,1,len(rows)-2)])
def compose(name, header, blocks):
    H=max(len(b[0]) for b in blocks)
    out=['']*H; probes=[]; x=0
    for rows,pr in blocks:
        w=len(rows[0]); pad=H-len(rows)
        for i in range(H): out[i]+=(rows[i-pad] if i>=pad else '.'*w)+'..'
        probes+= [(l,x+px,pad+py) for l,px,py in pr]
        x+=w+2
    lines=['labels = on']+header+[f'probe.{l} = {px},{py}' for l,px,py in probes]+['']+[r.rstrip('.') or '.' for r in out]
    open(f'boards/{name}.txt','w').write('\n'.join(lines)+'\n'); print(name,len(probes),'probes')

def cross_gate(g,label):
    return block(['......L','......W','......5','......R','.'+g*2+'+'+g*3,'.W','.R','EEE','EEE','EEE'],[(label,1,8)])
a=[
 block(['....l','....W','....9','LW22+2222','....9...2','....R...R','...EEE.EEE','...EEE.EEE','...EEE.EEE'],[('cross_vertical_off',4,7),('cross_horizontal_on',8,7)]),
 block(['LW2+9','....9','....R','...EEE','...EEE','...EEE'],[('cross_two_colours',4,4)]),
 block(['...L','...W','lW5+5555','...R...R','..EEE.EEE','..EEE.EEE','..EEE.EEE'],[('cross_write_read_on',3,5),('cross_other_off',7,5)]),
 cross_gate('O','or_through_cross'), cross_gate('X','xor_through_cross'), cross_gate('Z','xnor_through_cross'), cross_gate('A','and_through_cross'),
 block(['LW2T.lW9T.T2..T9','...........R...R','..........EEE.EEE','..........EEE.EEE','..........EEE.EEE'],[('tunnel_red_on',11,3),('tunnel_yellow_off',15,3)]),
 col('LW5RRW5R','read_write_inside_trace'),
 col('LW5REW5R','led_drives_led'),
]
compose('routing-a',['ticks = 0..4','covers = rule4, rule5, rule8, Cross, Tunnel, 12'],a)

def bus_gate(g,label):
    return block(['.....L','.....W','.....R','.'+g+'uuu'+g,'.W','.R','EEE','EEE','EEE'],[(label,1,7)])
b=[
 block(['L.l','W.W','2.9','uuuuuvvvvvv','.....9...2','.....R...R','....EEE.EEE','....EEE.EEE','....EEE.EEE'],[('bus_nine_off',5,7),('bus_two_on',9,7)]),
 block(['L','W','2','9','wwwwwxxxxxx','.....2...9','.....R...R','....EEE.EEE','....EEE.EEE','....EEE.EEE'],[('bus_touching_colour_differs',5,8),('bus_touching_colour_same',9,8)]),
 block(['L','W','2','yyy+zzz','.....2','.....R','....EEE','....EEE','....EEE'],[('bus_through_cross',5,7)]),
 block(['L','W','2','uuT..Tuu','.......2','.......R','......EEE','......EEE','......EEE'],[('bus_through_tunnel',7,7)]),
 bus_gate('A','and_through_bus'), bus_gate('X','xor_through_bus'), bus_gate('N','not_through_bus'),
 block(['LW2M.M2..M9','......R...R','.....EEE.EEE','.....EEE.EEE','.....EEE.EEE'],[('mesh_same_colour',6,3),('mesh_other_colour',10,3)]),
 block(['L','W','5..g...h','R..W...W','g..5...5','...R...R','..EEE.EEE','..EEE.EEE','..EEE.EEE'],[('wireless_0_on',3,7),('wireless_1_off',7,7)]),
]
compose('routing-b',['ticks = 0..4','covers = rule3, rule6, rule7, Bus, Mesh, Wireless, 12'],b)

two=lambda top: [a+'.'+b for a,b in top]+['lll','.W.','.5.','.R.','EEE','EEE','EEE']
g=[
 col('CW5R','clock'),
 col('LW5RCW5R','clock_with_input'),
 block(two(list(zip('LW'+'5'*9+'R','LW5RBW5RBW5R'))),[('latch_second_rise',1,17)]),
 block(two(list(zip('LW5RNW5R','LW5RBW5R'))),[('latch_fall_and_rise',1,13)]),
 block(['.L.','lW.','.5.','.R.','EEE','EEE','EEE'],[('shared_write_a',1,5)]),
 block(['.l.','LW.','.5.','.R.','EEE','EEE','EEE'],[('shared_write_b',1,5)]),
 block(['.'+c+'.' for c in 'LW5R'+'BW5R'*6][:-1]+['!R.','EEE','EEE','EEE'],[('breakpoint_chain',1,29)]),
]
compose('general',['clock_interval = 3','ticks = 0..10','covers = Clock, Breakpoint, Latch, 11, 13, 17, 20'],g)
compose('general-1',['clock_interval = 1','ticks = 0..5','covers = Clock, 13'],[col('CW5R','clock_interval_1')])
