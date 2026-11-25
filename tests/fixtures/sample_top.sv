// module fake (input x);  <- comment, must not be a module
`timescale 1ns/1ps
module sub_ni (a, b, q);
  input a;
  input [3:0] b;
  output reg q;
endmodule

module top #(parameter W = 8) (
  input  wire        clk,
  input  logic [W-1:0] din, din2,
  output logic [W-1:0] dout,
  inout  wire        io
);
  wire [W-1:0] s1, s2;
  logic ready;

  sub_ni u_named (.a(clk), .b(din[3:0]), .q(ready));
  sub_ni u_pos (clk, din[7:4], s1[0]);
  sub_ni u_unconn (.a(clk), .b(), .q(ready));
  other_mod #(.W(W)) u_other (.x(s1), .y(s2));

  /* module ghost_block (); */
  genvar i;
  generate
    for (i = 0; i < 2; i = i + 1) begin : g
      sub_ni u_gen (.a(clk), .b(din[3:0]), .q());
    end
  endgenerate

  assign dout = s2;
endmodule
