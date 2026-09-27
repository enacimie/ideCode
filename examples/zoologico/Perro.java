public class Perro extends Animal implements Mascota {
    private String raza;

    public Perro(String nombre, int edad, String raza) {
        super(nombre, edad);
        this.raza = raza;
    }

    @Override
    public String hablar() {
        return "¡Guau!";
    }

    @Override
    public void jugar() {
        System.out.println(nombre + " corre tras la pelota.");
    }

    public String getRaza() {
        return raza;
    }
}