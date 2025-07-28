
symbol ld  0b0010_0000_0000_0000
symbol lia 0b0011_0000_0000_0000
symbol lib 0b0100_0000_0000_0000
symbol sto 0b0101_0000_0000_0000

symbol sum    0b0110_0100_0000_0000
symbol sum_32 0b0110_1100_0000_0000
symbol sub    0b0110_0010_0000_0000
symbol sub_32 0b0110_1010_0000_0000
symbol shf    0b0110_0001_0000_0000
symbol shf_32 0b0110_1001_0000_0000
symbol and    0b0110_0000_1000_0000
symbol and_32 0b0110_1000_1000_0000
symbol or     0b0110_0000_0100_0000
symbol or_32  0b0110_1000_0100_0000
symbol xor    0b0110_0000_0010_0000
symbol xor_32 0b0110_1000_0010_0000
symbol neg    0b0110_0000_0001_0000
symbol neg_32 0b0110_1000_0001_0000

symbol psh 0b1010_0000_0000_0000
symbol pop 0b1011_0000_0000_0000

symbol jmp  0b1100_0000_0000_0000
symbol call 0b1110_0000_0000_0000
symbol ret  0b1111_0000_0000_0000


symbol rgAH 0b1000_0000_0000
symbol rgA  0b0100_0000_0000
symbol rgBH 0b0010_0000_0000
symbol rgB  0b0001_0000_0000
symbol rgSP 0b0000_1000_0000
symbol rgSB 0b0000_0100_0000
symbol non  0

symbol zero  0b1000_0000_0000
symbol carry 0b0100_0000_0000
symbol nega  0b0010_0000_0000
symbol eq    0b0001_0000_0000
symbol neq   0b0000_1000_0000

pointer a 0x1001 0xAAAA;
pointer b 0x1002 0xBBBB;

pointer x 0x1003 0xCCCC;
pointer y 0x1004 0xDDDD;

pointer o 0x1005 0x0001;
pointer p 0x1006 0xFFFF;



origin 0xFF00
call; DB;
jmp; DB;
#ld rgAH; o;
#ld rgA; o;
#ld rgBH; o;
ld rgB; o;

sub;
shf 15;

lia 5;
lib 1;

@ Loop1
sub;
jmp zero; End;
jmp; Loop1;

@ End

lia 1;
lib 1;

@ Loop
psh rgA;
psh rgAH;
sum;
pop rgBH;
pop rgB;
jmp; Loop;

@ DB
ret;
lia 173;
psh rgA;
lia 0;
@ Start
psh rgA;
pop rgAH;
pop rgA;
shf 1;
psh rgA;
psh rgAH;
pop rgA;
jmp eq; Start;




