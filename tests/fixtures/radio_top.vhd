LIBRARY IEEE,vhdl_pkgs;
  USE IEEE.STD_LOGIC_1164.ALL;
  USE IEEE.numeric_std.ALL;
  USE IEEE.std_logic_unsigned.ALL;
  USE std.textio.ALL;
  USE IEEE.MATH_REAL.ALL;
  USE vhdl_pkgs.tb_pkg.ALL;

-- vhdl_ls off
LIBRARY xpm;
-- vhdl_ls on

ENTITY radio_top IS
  -- GENERIC(

  -- );
  PORT(
    aresetn           : IN  STD_ULOGIC;
    inputs            : IN  block_inputs_t;
    outputs           : OUT block_outputs_t;

    -- IQ data input
    iq_data_in        : IN STD_ULOGIC_VECTOR(31 DOWNTO 0);
    iq_data_in_valid  : IN STD_ULOGIC;

    -- IQ data output
    iq_data_out        : OUT STD_ULOGIC_VECTOR(31 DOWNTO 0);
    iq_data_out_valid  : OUT STD_ULOGIC


  );
END ENTITY radio_top;

ARCHITECTURE struct OF radio_top IS

--ANCHOR - Constants
CONSTANT C_S00_AXI_ADDR_WIDTH : INTEGER := 7;
CONSTANT C_S00_AXI_DATA_WIDTH : INTEGER := 32;

--ANCHOR - Signals
SIGNAL bram_addr_tb               : STD_ULOGIC_VECTOR(9 DOWNTO 0);
SIGNAL phy_tx_started_tb          : STD_ULOGIC := '0';
SIGNAL phy_tx_done_tb             : STD_ULOGIC := '0';
SIGNAL data_addr_tb               : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0');
SIGNAL data_in_tb                 : STD_ULOGIC_VECTOR(63 DOWNTO 0) := (OTHERS => '0');
SIGNAL data_wen_tb                : STD_ULOGIC_VECTOR(7 DOWNTO 0)  := (OTHERS => '0');
SIGNAL result_i_tb                : STD_ULOGIC_VECTOR(15 DOWNTO 0) := (OTHERS => '0');
SIGNAL result_q_tb                : STD_ULOGIC_VECTOR(15 DOWNTO 0) := (OTHERS => '0');
SIGNAL result_iq_valid_tb         : STD_ULOGIC;
SIGNAL mode_1r1t_tb               : STD_ULOGIC := '1'; 

--ANCHOR - tx_ctrl signals
SIGNAL addr_abs_tb                : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0');
SIGNAL start_tb                   : STD_ULOGIC := '0';
SIGNAL i_out_tb                   : STD_ULOGIC_VECTOR(15 DOWNTO 0) := (OTHERS => '0');
SIGNAL q_out_tb                   : STD_ULOGIC_VECTOR(15 DOWNTO 0) := (OTHERS => '0');
SIGNAL iq_valid_out_tb            : STD_ULOGIC := '0';
SIGNAL tx_hold_tb                 : STD_ULOGIC := '0';
SIGNAL we_done_tb                 : STD_ULOGIC_VECTOR(7 DOWNTO 0);
SIGNAL mac_cts_tb                 : STD_ULOGIC := '0';
SIGNAL mac_tx_req_tb              : STD_ULOGIC := '0';
SIGNAL mac_tx_end_tb              : STD_ULOGIC := '0';
SIGNAL single_ant_tb              : STD_ULOGIC := '0';


SIGNAL debug_current_buffer_tb    : STD_ULOGIC_VECTOR(2 DOWNTO 0)  := (OTHERS => '0');
SIGNAL debug_data_buffer_valid_tb : STD_ULOGIC_VECTOR(7 DOWNTO 0)  := (OTHERS => '0');
SIGNAL bram_din_tb                : STD_ULOGIC_VECTOR(63 DOWNTO 0) := (OTHERS => '0');


-- AXI4-Lite Write Address Channel
SIGNAL s00_axi_awaddr_tb  : STD_ULOGIC_VECTOR(C_S00_AXI_ADDR_WIDTH-1 DOWNTO 0);
SIGNAL s00_axi_awprot_tb  : STD_ULOGIC_VECTOR(2 DOWNTO 0);
SIGNAL s00_axi_awvalid_tb : STD_ULOGIC;
SIGNAL s00_axi_awready_tb : STD_ULOGIC;

-- AXI4-Lite Write Data Channel
SIGNAL s00_axi_wdata_tb  : STD_ULOGIC_VECTOR(C_S00_AXI_DATA_WIDTH-1 DOWNTO 0);
SIGNAL s00_axi_wstrb_tb  : STD_ULOGIC_VECTOR((C_S00_AXI_DATA_WIDTH/8)-1 DOWNTO 0);
SIGNAL s00_axi_wvalid_tb : STD_ULOGIC;
SIGNAL s00_axi_wready_tb : STD_ULOGIC;

-- AXI4-Lite Write Response Channel
SIGNAL s00_axi_bresp_tb  : STD_ULOGIC_VECTOR(1 DOWNTO 0);
SIGNAL s00_axi_bvalid_tb : STD_ULOGIC;
SIGNAL s00_axi_bready_tb : STD_ULOGIC;

-- AXI4-Lite Read Address Channel
SIGNAL s00_axi_araddr_tb  : STD_ULOGIC_VECTOR(C_S00_AXI_ADDR_WIDTH-1 DOWNTO 0);
SIGNAL s00_axi_arprot_tb  : STD_ULOGIC_VECTOR(2 DOWNTO 0);
SIGNAL s00_axi_arvalid_tb : STD_ULOGIC;
SIGNAL s00_axi_arready_tb : STD_ULOGIC;

-- AXI4-Lite Read Data Channel
SIGNAL s00_axi_rdata_tb  : STD_ULOGIC_VECTOR(C_S00_AXI_DATA_WIDTH-1 DOWNTO 0);
SIGNAL s00_axi_rresp_tb  : STD_ULOGIC_VECTOR(1 DOWNTO 0);
SIGNAL s00_axi_rvalid_tb : STD_ULOGIC;
SIGNAL s00_axi_rready_tb : STD_ULOGIC;


-- Block memory
SIGNAL ena_tb   : STD_ULOGIC := '0';
SIGNAL wea_tb   : STD_ULOGIC_VECTOR(7 DOWNTO 0)  := (OTHERS => '0');
SIGNAL addra_tb : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0');
SIGNAL douta_tb : STD_ULOGIC_VECTOR(63 DOWNTO 0) := (OTHERS => '0');
SIGNAL clka_tb  : STD_ULOGIC := '0';
SIGNAL rstb_tb  : STD_ULOGIC := '0';
SIGNAL enb_tb   : STD_ULOGIC := '0';
SIGNAL web_tb   : STD_ULOGIC_VECTOR(7 DOWNTO 0)  := (OTHERS => '0');
SIGNAL addrb_tb : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0');
SIGNAL dinb_tb  : STD_ULOGIC_VECTOR(63 DOWNTO 0) := (OTHERS => '0');
SIGNAL doutb_tb : STD_ULOGIC_VECTOR(63 DOWNTO 0) := (OTHERS => '0'); 

SIGNAL byte_in_tb      : STD_ULOGIC_VECTOR(7 DOWNTO 0);
SIGNAL byte_valid_tb   : STD_ULOGIC;
SIGNAL buffer_index_tb : STD_ULOGIC_VECTOR(2 DOWNTO 0);

SIGNAL bram_we_tb      : STD_ULOGIC_VECTOR(3 DOWNTO 0)  := (OTHERS => '0');
SIGNAL bram_en_tb      : STD_ULOGIC;

--ANCHOR - Dac mux mult signals
SIGNAL dac_i_out_tb : STD_ULOGIC_VECTOR(15 DOWNTO 0);
SIGNAL dac_q_out_tb : STD_ULOGIC_VECTOR(15 DOWNTO 0);
SIGNAL sat_i_tb     : STD_ULOGIC;
SIGNAL sat_q_tb     : STD_ULOGIC;


--ANCHOR - openofdm rx signals
SIGNAL rssi_half_db_tb            : STD_ULOGIC_VECTOR(10 DOWNTO 0) := (OTHERS => '0');
SIGNAL state_out_tb               : STD_ULOGIC_VECTOR(4 DOWNTO 0);
SIGNAL watchdog_rst_out_tb        : STD_ULOGIC;
SIGNAL power_trigger_out_tb       : STD_ULOGIC;
SIGNAL demod_is_ongoing_tb        : STD_ULOGIC;
SIGNAL byte_out_tb                : STD_ULOGIC_VECTOR(7 DOWNTO 0);
SIGNAL byte_count_tb              : STD_ULOGIC_VECTOR(15 DOWNTO 0);
SIGNAL byte_out_strobe_tb         : STD_ULOGIC;
SIGNAL fcs_out_strobe_tb          : STD_ULOGIC;
SIGNAL short_preamble_detected_tb : STD_ULOGIC;
SIGNAL long_preamble_detected_tb  : STD_ULOGIC;

--ANCHOR - Loopback signals
SIGNAL iq_data_out_lb_tb : STD_ULOGIC_VECTOR(31 DOWNTO 0);
SIGNAL valid_out_lb_tb   : STD_ULOGIC;

--ANCHOR - RSSI signals
-- declare dummies with correct widths
SIGNAL iq_rssi_half_db_tb                : STD_ULOGIC_VECTOR(8 DOWNTO 0)  := (OTHERS => '0');
SIGNAL rssi_half_db_lock_by_sig_valid_tb : STD_ULOGIC_VECTOR(10 DOWNTO 0) := (OTHERS => '0');
SIGNAL gpio_status_lock_by_sig_valid_tb  : STD_ULOGIC_VECTOR(7 DOWNTO 0)  := (OTHERS => '0');
SIGNAL gpio_status_delay_tb              : STD_ULOGIC_VECTOR(7 DOWNTO 0)  := (OTHERS => '0');

--ANCHOR -  rx_ctrl
SIGNAL rx_bram_addr_tb            : STD_ULOGIC_VECTOR(31 DOWNTO 0);
SIGNAL rx_bram_din_tb             : STD_ULOGIC_VECTOR(31 DOWNTO 0);
SIGNAL rx_bram_dout_tb            : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0');
SIGNAL rx_bram_we_tb              : STD_ULOGIC_VECTOR(3 DOWNTO 0);
SIGNAL rx_bram_en_tb              : STD_ULOGIC;
SIGNAL rx_irq_tb                  : STD_ULOGIC;
SIGNAL fcs_ok_tb                  : STD_ULOGIC;
SIGNAL pkt_header_valid_strobe_tb : STD_ULOGIC;
SIGNAL pkt_header_valid_tb        : STD_ULOGIC;

--ANCHOR -  media_acc_ctrl
SIGNAL rx_en_tb            : STD_ULOGIC := '0';
SIGNAL lna_en_tb           : STD_ULOGIC := '0';
SIGNAL tx_en_tb            : STD_ULOGIC := '0';
SIGNAL pa_en_tb            : STD_ULOGIC := '0';
SIGNAL rf_switch_tb        : STD_ULOGIC_VECTOR(5 DOWNTO 0) := (OTHERS => '0');

--ANCHOR -  timestamp
SIGNAL timestamp_tb   : STD_LOGIC_VECTOR(63 DOWNTO 0);


--ANCHOR - AXI4 debug block parameters
CONSTANT C_AXI_ID_WIDTH   : INTEGER := 3;
CONSTANT C_AXI_ADDR_WIDTH : INTEGER := 24;
CONSTANT C_AXI_DATA_WIDTH : INTEGER := 32;
CONSTANT C_AXI_USER_WIDTH : INTEGER := 5;

-- AXI4 Write Address Channel
SIGNAL S_AXI_AWID_tb    : STD_ULOGIC_VECTOR(C_AXI_ID_WIDTH-1 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_AWADDR_tb  : STD_ULOGIC_VECTOR(C_AXI_ADDR_WIDTH-1 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_AWLEN_tb   : STD_ULOGIC_VECTOR(7 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_AWSIZE_tb  : STD_ULOGIC_VECTOR(2 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_AWBURST_tb : STD_ULOGIC_VECTOR(1 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_AWLOCK_tb  : STD_ULOGIC := '0';
SIGNAL S_AXI_AWCACHE_tb : STD_ULOGIC_VECTOR(3 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_AWPROT_tb  : STD_ULOGIC_VECTOR(2 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_AWQOS_tb   : STD_ULOGIC_VECTOR(3 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_AWUSER_tb  : STD_ULOGIC_VECTOR(0 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_AWVALID_tb : STD_ULOGIC := '0';
SIGNAL S_AXI_AWREADY_tb : STD_ULOGIC := '0';

-- AXI4 Write Data Channel
SIGNAL S_AXI_WID_tb     : STD_ULOGIC_VECTOR(C_AXI_ID_WIDTH-1 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_WDATA_tb   : STD_ULOGIC_VECTOR(C_AXI_DATA_WIDTH-1 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_WSTRB_tb   : STD_ULOGIC_VECTOR((C_AXI_DATA_WIDTH/8)-1 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_WLAST_tb   : STD_ULOGIC := '0';
SIGNAL S_AXI_WVALID_tb  : STD_ULOGIC := '0';
SIGNAL S_AXI_WREADY_tb  : STD_ULOGIC := '0';

-- AXI4 Write Response Channel
SIGNAL S_AXI_BID_tb     : STD_ULOGIC_VECTOR(C_AXI_ID_WIDTH-1 DOWNTO 0);
SIGNAL S_AXI_BRESP_tb   : STD_ULOGIC_VECTOR(1 DOWNTO 0);
SIGNAL S_AXI_BVALID_tb  : STD_ULOGIC := '0';
SIGNAL S_AXI_BREADY_tb  : STD_ULOGIC := '0';

-- AXI4 Read Address Channel
SIGNAL S_AXI_ARID_tb    : STD_ULOGIC_VECTOR(C_AXI_ID_WIDTH-1 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_ARADDR_tb  : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_ARLEN_tb   : STD_ULOGIC_VECTOR(7 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_ARSIZE_tb  : STD_ULOGIC_VECTOR(2 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_ARBURST_tb : STD_ULOGIC_VECTOR(1 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_ARLOCK_tb  : STD_ULOGIC := '0';
SIGNAL S_AXI_ARCACHE_tb : STD_ULOGIC_VECTOR(3 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_ARPROT_tb  : STD_ULOGIC_VECTOR(2 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_ARQOS_tb   : STD_ULOGIC_VECTOR(3 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_ARUSER_tb  : STD_ULOGIC_VECTOR(C_AXI_USER_WIDTH-1 DOWNTO 0) := (OTHERS => '0');
SIGNAL S_AXI_ARVALID_tb : STD_ULOGIC := '0';
SIGNAL S_AXI_ARREADY_tb : STD_ULOGIC := '0';

-- AXI4 Read Data Channel
SIGNAL S_AXI_RID_tb     : STD_ULOGIC_VECTOR(C_AXI_ID_WIDTH-1 DOWNTO 0);
SIGNAL S_AXI_RDATA_tb   : STD_ULOGIC_VECTOR(C_AXI_DATA_WIDTH-1 DOWNTO 0);
SIGNAL S_AXI_RRESP_tb   : STD_ULOGIC_VECTOR(1 DOWNTO 0);
SIGNAL S_AXI_RLAST_tb   : STD_ULOGIC := '0';
SIGNAL S_AXI_RVALID_tb  : STD_ULOGIC := '0';
SIGNAL S_AXI_RREADY_tb  : STD_ULOGIC := '0';

--ANCHOR - Debug registers
SIGNAL tx_ctrl_reg_1_tb   : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0'); --
SIGNAL tx_ctrl_reg_2_tb   : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0'); -- 
SIGNAL ofdm_rx_reg_tb     : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0'); --
SIGNAL rx_ctrl_reg_1_tb   : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0'); --
SIGNAL rx_ctrl_reg_2_tb   : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0'); --
SIGNAL rx_ctrl_reg_3_tb   : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0'); --
SIGNAL rx_ctrl_reg_4_tb   : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0'); --
SIGNAL rssi_reg_tb        : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0'); --
SIGNAL dac_sample_reg_tb  : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0');
SIGNAL adc_sample_reg_tb  : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0');
SIGNAL ad9361_reg_tb      : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0');
SIGNAL timestamp_reg_1_tb : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0');
SIGNAL timestamp_reg_2_tb : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0');
SIGNAL wdg_event_cntr_tb  : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0'); --
SIGNAL last_read_reg_tb   : STD_ULOGIC_VECTOR(31 DOWNTO 0) := (OTHERS => '0');

-- Writeable registers
SIGNAL wr_tx_ctrl_reg_out_tb   : STD_ULOGIC_VECTOR(31 DOWNTO 0); -- 
SIGNAL wr_ofdm_tx_reg_out_tb   : STD_ULOGIC_VECTOR(31 DOWNTO 0); --
SIGNAL wr_rx_ctrl_reg_out_tb   : STD_ULOGIC_VECTOR(31 DOWNTO 0); --
SIGNAL wr_ofdm_wdg_reg_out_tb  : STD_ULOGIC_VECTOR(31 DOWNTO 0); --
SIGNAL wr_dot11_reg_1_out_tb   : STD_ULOGIC_VECTOR(31 DOWNTO 0); --
SIGNAL wr_dot11_reg_2_out_tb   : STD_ULOGIC_VECTOR(31 DOWNTO 0); --
SIGNAL wr_rssi_reg_out_tb      : STD_ULOGIC_VECTOR(31 DOWNTO 0); --
SIGNAL wr_wdg_event_reg_out_tb : STD_ULOGIC_VECTOR(31 DOWNTO 0); --
SIGNAL wr_mac_ctrl_reg1_out_tb : STD_ULOGIC_VECTOR(31 DOWNTO 0); --
SIGNAL wr_mac_ctrl_reg2_out_tb : STD_ULOGIC_VECTOR(31 DOWNTO 0); --

BEGIN

  -- vhdl_ls off
    --ANCHOR - tx_ctrl_inst
    tx_ctrl_inst : ENTITY work.tx_ctrl
    PORT MAP (
        -- Inputs
        clk                     => inputs.div_clk,
        aresetn                 => aresetn,
        addr_rel                => bram_addr_tb,
        started                 => phy_tx_started_tb,
        done                    => phy_tx_done_tb,

        fpga_clk                => inputs.fpga_clk,
        fpga_aresetn            => aresetn,
        data_addr               => inputs.tx_addra,
        data_in                 => inputs.tx_dina,
        data_wen                => inputs.tx_wea,
        in_tx_ctrl_reg          => wr_tx_ctrl_reg_out_tb,

        i                       => result_i_tb,
        q                       => result_q_tb,
        iq_valid                => result_iq_valid_tb,
        mode_1r1t               => mode_1r1t_tb,

        mac_cts                 => mac_cts_tb,    

        -- Outputs
        addr_abs                => addr_abs_tb,
        start                   => start_tb,
        i_out                   => i_out_tb,
        q_out                   => q_out_tb,
        iq_out_lb               => iq_data_out,
        iq_out_lb_valid         => iq_data_out_valid,
        iq_out_valid            => iq_valid_out_tb,
        out_tx_ctrl_reg_1       => tx_ctrl_reg_1_tb,
        out_tx_ctrl_reg_2       => tx_ctrl_reg_2_tb,
        tx_hold                 => tx_hold_tb,
        we_done                 => we_done_tb,

        mac_tx_req              => mac_tx_req_tb,
        mac_tx_end              => mac_tx_end_tb        
    );

    --ANCHOR - openofdm tx inst
    openofdm_tx_inst : ENTITY work.openofdm_tx
    GENERIC MAP (
        C_S00_AXI_DATA_WIDTH => C_S00_AXI_DATA_WIDTH,
        C_S00_AXI_ADDR_WIDTH => C_S00_AXI_ADDR_WIDTH 
    )
    PORT MAP (
        -- inputs
        clk                 => inputs.fpga_clk,
        phy_tx_arestn       => aresetn,
        phy_tx_start        => start_tb,
        bram_din            => doutb_tb,
        result_iq_hold      => tx_hold_tb,
        s00_axi_aclk        => inputs.fpga_clk,
        s00_axi_aresetn     => aresetn,
        timestamp           => timestamp_tb,
        ofdm_tx_reg         => wr_ofdm_tx_reg_out_tb,

        -- outputs
        phy_tx_done         => phy_tx_done_tb,
        phy_tx_started      => phy_tx_started_tb,
        bram_addr           => bram_addr_tb,
        result_iq_valid     => result_iq_valid_tb,
        result_i            => result_i_tb,
        result_q            => result_q_tb,

        --s00_axi
        s00_axi_awaddr      =>  s00_axi_awaddr_tb,
        s00_axi_awprot      =>  s00_axi_awprot_tb,
        s00_axi_awvalid     =>  s00_axi_awvalid_tb,
        s00_axi_awready     =>  s00_axi_awready_tb,
        s00_axi_wdata       =>  s00_axi_wdata_tb,
        s00_axi_wstrb       =>  s00_axi_wstrb_tb,
        s00_axi_wvalid      =>  s00_axi_wvalid_tb,
        s00_axi_wready      =>  s00_axi_wready_tb,
        s00_axi_bresp       =>  s00_axi_bresp_tb,
        s00_axi_bvalid      =>  s00_axi_bvalid_tb,
        s00_axi_bready      =>  s00_axi_bready_tb,
        s00_axi_araddr      =>  s00_axi_araddr_tb,
        s00_axi_arprot      =>  s00_axi_arprot_tb,
        s00_axi_arvalid     =>  s00_axi_arvalid_tb,
        s00_axi_arready     =>  s00_axi_arready_tb,
        s00_axi_rdata       =>  s00_axi_rdata_tb,
        s00_axi_rresp       =>  s00_axi_rresp_tb,
        s00_axi_rvalid      =>  s00_axi_rvalid_tb,
        s00_axi_rready      =>  s00_axi_rready_tb
    );

    blk_mem_gen_inst : ENTITY work.blk_mem_gen_1
    PORT MAP (
      clka  => inputs.fpga_clk,
      rsta  => NOT aresetn,
      ena   => inputs.tx_ena,
      wea   => inputs.tx_wea,
      addra => inputs.tx_addra,
      dina  => inputs.tx_dina,
      douta => douta_tb,
      clkb  => inputs.fpga_clk,
      rstb  => '0',
      enb   => '1',
      web   => we_done_tb,
      addrb => addr_abs_tb,
      dinb  => (OTHERS => '0'),
      doutb => doutb_tb
    );

    dac_mux_mult_inst : ENTITY work.dac_mux_mult
    PORT MAP (
        openofdm_en        => '1',
        dac_i_sw           => (OTHERS => '0'),
        dac_q_sw           => (OTHERS => '0'),
        dac_i_openofdm     => i_out_tb,
        dac_q_openofdm     => q_out_tb,
        dac_i_out          => dac_i_out_tb,
        dac_q_out          => dac_q_out_tb
    );

    --ANCHOR - openofdm rx inst
open_ofdm_rx_inst : ENTITY work.openofdm_rx
    GENERIC MAP (
        IQ_DATA_WIDTH          => 16,
        RSSI_HALF_DB_WIDTH     => 11,
        WATCHDOG_COUNTER_WIDTH => 22
    )
    PORT MAP (
        -- Control and parameter inputs
        
        -- Interface to dot11
        rssi_half_db     => rssi_half_db_tb,
        sample_in        => iq_data_in,
        sample_in_strobe => iq_data_in_valid,

        in_ofdm_wdg_reg  => wr_ofdm_wdg_reg_out_tb,
        in_dot11_reg_1   => wr_dot11_reg_1_out_tb,
        in_dot11_reg_2   => wr_dot11_reg_2_out_tb,
        in_wdg_event_reg => wr_wdg_event_reg_out_tb,

        out_ofdm_rx_reg  => ofdm_rx_reg_tb,
        out_wdg_cntr_reg => wdg_event_cntr_tb,
        
        -- Demodulation status outputs
        power_trigger_out       => power_trigger_out_tb,
        demod_is_ongoing        => demod_is_ongoing_tb,
        short_preamble_detected => short_preamble_detected_tb,
        long_preamble_detected  => long_preamble_detected_tb,
        
        -- Packet header outputs
        pkt_header_valid        => pkt_header_valid_tb,
        pkt_header_valid_strobe => pkt_header_valid_strobe_tb,
        ht_unsupport            => open,
        pkt_rate                => open,
        pkt_len                 => open,
        ht_aggr                 => open,
        ht_aggr_last            => open,
        ht_sgi                  => open,
        
        -- Byte output interface
        byte_out_strobe => byte_out_strobe_tb,
        byte_out        => byte_out_tb,
        byte_count      => byte_count_tb,
        
        -- FCS outputs
        fcs_out_strobe => fcs_out_strobe_tb,
        fcs_ok         => fcs_ok_tb,
        
        -- Side channel outputs
        csi                      => open,
        csi_valid                => open,
        phase_offset_taken       => open,
        equalizer                => open,
        equalizer_valid          => open,
        ofdm_symbol_eq_out_pulse => open,
        
        -- PHY length info outputs
        n_ofdm_sym        => open,
        n_bit_in_last_sym => open,
        phy_len_valid     => open,
        state_out         => state_out_tb,
        watchdog_rst_out  => watchdog_rst_out_tb,
        
        -- AXI4-Lite interface
        s00_axi_aclk    => inputs.wiz_clk,
        s00_axi_aresetn => aresetn
    );


  --ANCHOR - rssi inst
  rssi_inst : ENTITY work.rssi
    GENERIC MAP (
      GPIO_STATUS_WIDTH     => 8,
      DELAY_CTL_WIDTH       => 7,
      RSSI_HALF_DB_WIDTH    => 11,
      IQ_RSSI_HALF_DB_WIDTH => 9,
	    IQ_DATA_WIDTH	        => 16
    )
    PORT MAP (
        gpio_status                    => STD_LOGIC_VECTOR(TO_UNSIGNED(70, 8)),    -- [7:0]
        clk                            => inputs.wiz_clk,
        rstn                           => aresetn,
        fifo_delay_rstn                => aresetn,
        pkt_header_valid_strobe        => pkt_header_valid_strobe_tb,
        ddc_i                          => iq_data_out_lb_tb(31 DOWNTO 16),            -- [15:0]
        ddc_q                          => iq_data_out_lb_tb(15 DOWNTO 0),             -- [15:0]
        ddc_iq_valid                   => valid_out_lb_tb,
        in_rssi_reg                    => wr_rssi_reg_out_tb,
     
        iq_rssi_half_db                => iq_rssi_half_db_tb,
        iq_rssi_half_db_valid          => open,
        rssi_half_db_lock_by_sig_valid => rssi_half_db_lock_by_sig_valid_tb,
        gpio_status_lock_by_sig_valid  => gpio_status_lock_by_sig_valid_tb,
        rssi_half_db                   => rssi_half_db_tb,
        rssi_half_db_valid             => open,
        gpio_status_delay              => gpio_status_delay_tb,
        gpio_status_delay_valid        => open,
        out_rssi_reg                   => rssi_reg_tb
    );

  --ANCHOR - axi4_debug_ctrl inst
    axi4_debug_ctrl_inst: ENTITY work.axi4_debug_ctrl
    GENERIC MAP(
      BUSY_SLAVE_TEST => FALSE
    )
    PORT MAP(
      S_AXI_ACLK    => inputs.fpga_clk,
      S_AXI_ARESETN => aresetn,
      S_AXI_AWID    => S_AXI_AWID_tb, 
      S_AXI_AWVALID => S_AXI_AWVALID_tb,
      S_AXI_AWREADY => S_AXI_AWREADY_tb,
      S_AXI_AWADDR  => S_AXI_AWADDR_tb,
      S_AXI_AWLEN   => S_AXI_AWLEN_tb,
      S_AXI_AWSIZE  => S_AXI_AWSIZE_tb,
      S_AXI_AWBURST => S_AXI_AWBURST_tb,
      S_AXI_AWLOCK  => S_AXI_AWLOCK_tb,
      S_AXI_AWCACHE => S_AXI_AWCACHE_tb,
      S_AXI_AWPROT  => S_AXI_AWPROT_tb,
      S_AXI_AWQOS   => S_AXI_AWQOS_tb,
      S_AXI_AWUSER  => S_AXI_AWUSER_tb,
      S_AXI_WID     => S_AXI_WID_tb,
      S_AXI_WVALID  => S_AXI_WVALID_tb,
      S_AXI_WREADY  => S_AXI_WREADY_tb,
      S_AXI_WDATA   => S_AXI_WDATA_tb,
      S_AXI_WSTRB   => S_AXI_WSTRB_tb,
      S_AXI_WLAST   => S_AXI_WLAST_tb,
      S_AXI_BID     => S_AXI_BID_tb,
      S_AXI_BVALID  => S_AXI_BVALID_tb,
      S_AXI_BREADY  => S_AXI_BREADY_tb,
      S_AXI_BRESP   => S_AXI_BRESP_tb,
      S_AXI_ARID    => S_AXI_ARID_tb,
      S_AXI_ARVALID => S_AXI_ARVALID_tb,
      S_AXI_ARREADY => S_AXI_ARREADY_tb,
      S_AXI_ARADDR  => S_AXI_ARADDR_tb(31 DOWNTO 8),
      S_AXI_ARLEN   => S_AXI_ARLEN_tb,
      S_AXI_ARSIZE  => S_AXI_ARSIZE_tb,
      S_AXI_ARBURST => S_AXI_ARBURST_tb,
      S_AXI_ARLOCK  => S_AXI_ARLOCK_tb,
      S_AXI_ARCACHE => S_AXI_ARCACHE_tb,
      S_AXI_ARPROT  => S_AXI_ARPROT_tb,
      S_AXI_ARQOS   => S_AXI_ARQOS_tb,
      S_AXI_ARUSER  => S_AXI_ARUSER_tb(4 DOWNTO 4),
      S_AXI_RID     => S_AXI_RID_tb,
      S_AXI_RVALID  => S_AXI_RVALID_tb,
      S_AXI_RREADY  => S_AXI_RREADY_tb,
      S_AXI_RDATA   => S_AXI_RDATA_tb,
      S_AXI_RLAST   => S_AXI_RLAST_tb,
      S_AXI_RRESP   => S_AXI_RRESP_tb,

    -- Debug interface
    tx_ctrl_reg_1_in   => tx_ctrl_reg_1_tb,   
    tx_ctrl_reg_2_in   => tx_ctrl_reg_2_tb,   
    ofdm_rx_reg_in     => ofdm_rx_reg_tb,     
    rx_ctrl_reg_1_in   => rx_ctrl_reg_1_tb,   
    rx_ctrl_reg_2_in   => rx_ctrl_reg_2_tb,   
    rx_ctrl_reg_3_in   => rx_ctrl_reg_3_tb,   
    rx_ctrl_reg_4_in   => rx_ctrl_reg_4_tb,   
    rssi_reg_in        => rssi_reg_tb,        
    dac_sample_reg_in  => dac_sample_reg_tb,  
    adc_sample_reg_in  => adc_sample_reg_tb,  
    ad9361_reg_in      => ad9361_reg_tb,      
    timestamp_reg_1_in => timestamp_reg_1_tb, 
    timestamp_reg_2_in => timestamp_reg_2_tb, 
    wdg_event_cntr_in  => wdg_event_cntr_tb,  
    last_read_reg_in   => last_read_reg_tb,

    -- Writeable registers
    wr_tx_ctrl_reg_out => wr_tx_ctrl_reg_out_tb,   
    wr_ofdm_tx_reg_out => wr_ofdm_tx_reg_out_tb,   
    wr_rx_ctrl_reg_out => wr_rx_ctrl_reg_out_tb,   
    wr_ofdm_wdg_reg_out=> wr_ofdm_wdg_reg_out_tb,  
    wr_dot11_reg_1_out => wr_dot11_reg_1_out_tb,   
    wr_dot11_reg_2_out => wr_dot11_reg_2_out_tb,   
    wr_rssi_reg_out    => wr_rssi_reg_out_tb,
    wr_wdg_event_reg_out => wr_wdg_event_reg_out_tb,
    wr_mac_ctrl_reg1_out => wr_mac_ctrl_reg1_out_tb,
    wr_mac_ctrl_reg2_out => wr_mac_ctrl_reg2_out_tb  
    );


  rx_ctrl_inst: ENTITY work.rx_ctrl
    PORT MAP(
        ad_clk            => inputs.wiz_clk,
        aresetn           => aresetn,
        demod_on          => demod_is_ongoing_tb,
        byte_in           => byte_out_tb,
        byte_strobe       => byte_out_strobe_tb,
        fcs_ok            => fcs_ok_tb,
        fcs_strobe        => fcs_out_strobe_tb,
        timestamp         => timestamp_tb,
        rssi_half_db_lock => rssi_half_db_lock_by_sig_valid_tb,
        gpio_status_lock  => gpio_status_lock_by_sig_valid_tb,
        header_strobe     => pkt_header_valid_strobe_tb,
        header_ok         => pkt_header_valid_tb,
        short_preamb_det  => short_preamble_detected_tb,
        long_preamb_det   => long_preamble_detected_tb,
        in_rx_ctrl_reg    => wr_rx_ctrl_reg_out_tb,

        bram_addr => rx_bram_addr_tb,
        bram_din  => rx_bram_din_tb,
        bram_dout => rx_bram_dout_tb,
        bram_we   => rx_bram_we_tb,
        bram_en   => rx_bram_en_tb,
        irq       => rx_irq_tb,

        out_rx_ctrl_reg_1 => rx_ctrl_reg_1_tb,
        out_rx_ctrl_reg_2 => rx_ctrl_reg_2_tb,
        out_rx_ctrl_reg_3 => rx_ctrl_reg_3_tb,
        out_rx_ctrl_reg_4 => rx_ctrl_reg_4_tb
    );

  blk_mem_gen_inst_rx : ENTITY work.blk_mem_gen_2
    PORT MAP (
      clka  => inputs.fpga_clk,
      rsta  => NOT aresetn,
      ena   => rx_bram_en_tb,
      wea   => rx_bram_we_tb,
      addra => rx_bram_addr_tb,
      dina  => rx_bram_din_tb,
      douta => open,
      clkb  => inputs.rx_clkb,
      rstb  => inputs.rx_rstb,
      enb   => inputs.rx_enb,
      web   => inputs.rx_web,
      addrb => inputs.rx_addrb,
      dinb  => inputs.rx_dinb,
      doutb => open
    );

  --ANCHOR - media_acc_ctrl inst
  media_acc_ctrl_inst: ENTITY work.media_acc_ctrl
  PORT MAP(
    clk              => inputs.fpga_clk,
    aresetn          => aresetn,
    tx_req           => mac_tx_req_tb,
    tx_end           => mac_tx_end_tb,
    power_trig_ext   => power_trigger_out_tb,
    rssi_half_db     => rssi_half_db_tb,
    demod_on         => demod_is_ongoing_tb,
    in_mac_ctrl_reg1 => wr_mac_ctrl_reg1_out_tb,
    in_mac_ctrl_reg2 => wr_mac_ctrl_reg2_out_tb,  
    single_ant       => single_ant_tb,
    cts              => mac_cts_tb,        
    rx_en            => rx_en_tb,
    lna_en           => lna_en_tb,
    tx_en            => tx_en_tb,
    pa_en            => pa_en_tb,
    rf_switch        => rf_switch_tb

  );

    --ANCHOR - timestamp inst
  timestamp_inst : ENTITY work.timestamp
    GENERIC MAP (
      CYCLES_PER_TICK => 10
    )
    PORT MAP (
        clk       => inputs.fpga_clk,
        aresetn   => aresetn,
        timestamp => timestamp_tb
    );

  -- vhdl_ls on

END ARCHITECTURE struct;